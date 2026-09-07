import { expect, test } from "bun:test";
import type { Config, Hooks } from "@opencode-ai/plugin";
import type { AssistantMessage, Part, Session, SessionMessagesResponse, ToolPart, UserMessage } from "@opencode-ai/sdk";
import { createOpencodeClient } from "@opencode-ai/sdk";
import orchestrator from "../plugins/orchestrator";

const user = (providerID = "openai", id = "user") => ({
  info: {
    id, sessionID: "session", role: "user", agent: "build", time: { created: 1 },
    model: { providerID, modelID: "gpt-6-astra" },
  } satisfies UserMessage,
  parts: [],
});
const assistant = (providerID = "openai", parts: Part[] = []) => ({
  info: {
    id: "assistant", sessionID: "session", parentID: "user", role: "assistant", providerID,
    modelID: "gpt-6-astra", mode: "build", time: { created: 1 },
    path: { cwd: "/project", root: "/project" }, cost: 0,
    tokens: { input: 0, output: 0, reasoning: 0, cache: { read: 0, write: 0 } },
  } satisfies AssistantMessage,
  parts,
});

const commandPart: ToolPart = {
  id: "part", sessionID: "session", messageID: "assistant", type: "tool", tool: "task",
  callID: "different-call", state: { status: "running", input: {}, time: { start: 1 } },
};

const setup = async (messages: SessionMessagesResponse = [user(), assistant()]) => {
  const session: Session = {
    id: "session", projectID: "project", directory: "/project", title: "Test",
    version: "1", time: { created: 1, updated: 1 },
  };
  const state: { messages?: SessionMessagesResponse; session?: Session } = { messages, session };
  const requests: string[] = [];
  const client = createOpencodeClient({
    baseUrl: "http://opencode.test",
    fetch: async (request) => {
      const url = new URL(request.url);
      let data;
      switch (url.pathname) {
        case "/session/session":
          requests.push("get");
          data = state.session;
          break;
        case "/session/session/message": {
          const limit = Number(url.searchParams.get("limit"));
          requests.push(`messages:${limit}`);
          data = state.messages?.slice(-limit);
          break;
        }
        case "/session/session/message/user":
          requests.push("message:user");
          data = state.messages?.find(({ info }) => info.id === "user");
          break;
        default:
          throw new Error(`Unexpected request: ${request.method} ${url}`);
      }
      return data === undefined
        ? Response.json({ name: "NotFoundError", data: { message: `Missing ${url.pathname}` } }, { status: 404 })
        : Response.json(data);
    },
  });
  const hooks = await orchestrator({ client } as Parameters<typeof orchestrator>[0]);
  const task = (subagent_type: string, callID = "call", tool = "task") =>
    hooks["tool.execute.before"]({ tool, sessionID: "session", callID }, { args: { subagent_type } });
  return { hooks, state, requests, task };
};

test("registers worker defaults and preserves unrelated configuration", async () => {
  const general = { mode: "subagent" as const };
  const config: Config = { model: "openai/gpt-6-astra", agent: { general } };
  const { hooks } = await setup();
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.agent!.general).toBe(general);
  expect(Object.keys(config.agent!)).toEqual(["general", "luna", "sol", "astra"]);
  for (const [name, model, effort] of [
    ["luna", "openai/gpt-5.6-luna", "max"],
    ["sol", "openai/gpt-5.6-sol", "high"],
    ["astra", "openai/gpt-6-astra", "medium"],
  ]) {
    expect(config.agent![name]).toMatchObject({ mode: "subagent", model, options: { reasoningEffort: effort } });
    expect(config.agent![name].description).toBeString();
  }
});

test("preserves explicit worker permissions, disable, prompts, and model settings", async () => {
  const luna = {
    permission: { edit: "deny" as const }, disable: true, prompt: "Read only.",
    variant: "custom", model: "openai/custom-luna", mode: "all" as const,
    description: "Custom worker", options: { reasoningEffort: "low", custom: true },
  };
  const config: Config = { agent: { luna, sol: { options: { custom: true } } } };
  const { hooks } = await setup();
  await hooks.config(config);
  expect(config.agent!.luna).toEqual(luna);
  expect(config.agent!.sol.options).toEqual({ reasoningEffort: "high", custom: true });
  const registered = structuredClone(config);
  await hooks.config(config);
  expect(config).toEqual(registered);
});

test("adds conditional guidance only to task, without touching parameters", async () => {
  const { hooks, requests } = await setup();
  const output = { description: "Original task description", parameters: { original: true } };
  await hooks["tool.definition"]({ toolID: "task" }, output);
  expect(output.description).toStartWith("Original task description\n\nDelegation policy:");
  for (const text of [
    "top-level OpenAI", "do not use general", "legitimately available alternative",
    "most delegated work", "fresh perspective", "escalation does not require a separate user request",
    "defaults, not quotas", "non-overlapping writes", "reuse existing task IDs",
    "deliberate comparison and verification", "explicitly requests orchestration",
    "understand the implementation", "assess tradeoffs", "inspect key code",
  ]) {
    expect(output.description).toContain(text);
  }
  expect(output.parameters).toEqual({ original: true });
  const read = { description: "Read a file", parameters: {} };
  await hooks["tool.definition"]({ toolID: "read" }, read);
  expect(read).toEqual({ description: "Read a file", parameters: {} });
  expect(requests).toEqual([]);
});

test("internal LLM requests have no session lookup or system injection hooks", async () => {
  const { hooks } = await setup();
  // Title, compaction, and synthetic project-copy requests never receive task tools.
  for (const name of ["experimental.chat.system.transform", "chat.message", "chat.params", "event"] as const) {
    expect((hooks as Hooks)[name]).toBeUndefined();
  }
});

test("cold OpenAI continuation allows workers and denies general before tool parts exist", async () => {
  const { task } = await setup();
  for (const agent of ["luna", "sol", "astra"]) await expect(task(agent)).resolves.toBeUndefined();
  await expect(task("general")).rejects.toThrow("general is unavailable from OpenAI main sessions");
});

test("uses each executing assistant's provider rather than remembered or latest user models", async () => {
  const { task, state } = await setup();
  await expect(task("astra")).resolves.toBeUndefined();
  state.messages = [user(), assistant("other"), user("openai", "queued")];
  for (const agent of ["luna", "sol", "astra"]) {
    await expect(task(agent)).rejects.toThrow("available only from OpenAI main sessions");
  }
  await expect(task("general")).resolves.toBeUndefined();
  state.messages = [user("other"), assistant(), user("other", "queued")];
  await expect(task("astra")).resolves.toBeUndefined();
  await expect(task("general")).rejects.toThrow("unavailable from OpenAI main sessions");
});

test("child sessions cannot invoke workers but can use general", async () => {
  const { task, state } = await setup();
  state.session!.parentID = "parent";
  for (const agent of ["luna", "sol", "astra"]) {
    await expect(task(agent)).rejects.toThrow("available only from OpenAI main sessions");
  }
  await expect(task("general")).resolves.toBeUndefined();
});

test("unrelated tools and agent types need no lookups", async () => {
  const { task, requests } = await setup();
  await task("explore");
  await task("custom");
  await task("astra", "call", "read");
  expect(requests).toEqual([]);
});

test("finds the executing assistant behind queued users and shares its model across parallel tasks", async () => {
  const { task, state, requests } = await setup();
  state.messages!.push(...Array.from({ length: 5 }, (_, i) => user("other", `queued-${i}`)));
  await Promise.all([task("luna", "call-1"), task("sol", "call-2")]);
  expect(requests.sort()).toEqual(["get", "get", "messages:2", "messages:2", "messages:4", "messages:4", "messages:8", "messages:8"]);
});

test("command subtasks use their exact parent user's provider, not the target or queued model", async () => {
  const { task, state, requests } = await setup();
  state.messages = [user("other"), assistant("openai", [commandPart]), user("openai", "queued")];
  await expect(task("astra", "part")).rejects.toThrow("available only from OpenAI main sessions");
  await expect(task("general", "part")).resolves.toBeUndefined();
  state.messages = [user("openai"), assistant("other", [commandPart]), user("other", "queued")];
  await expect(task("astra", "part")).resolves.toBeUndefined();
  await expect(task("general", "part")).rejects.toThrow("unavailable from OpenAI main sessions");
  expect(requests.filter((request) => request.startsWith("message:"))).toEqual(Array(4).fill("message:user"));
});

test("command subtasks reuse an invoking user already in the fetched page", async () => {
  const { task, requests } = await setup([user(), assistant("other", [commandPart])]);
  await expect(task("astra", "part")).resolves.toBeUndefined();
  expect(requests).toEqual(["get", "messages:2"]);
});

test("normal task call IDs do not select the command authorization path", async () => {
  const { task, requests } = await setup([user("other"), assistant("openai", [commandPart])]);
  await expect(task("astra", commandPart.callID)).resolves.toBeUndefined();
  expect(requests).toEqual(["get", "messages:2"]);
});

test("SDK request errors retain their server diagnostics and HTTP status", async () => {
  const { task, state } = await setup();
  const session = state.session;
  state.session = undefined;
  await expect(task("astra")).rejects.toMatchObject({
    message: "Missing /session/session", cause: { status: 404 },
  });
  state.session = session;
  state.messages = undefined;
  await expect(task("general")).rejects.toMatchObject({
    message: "Missing /session/session/message", cause: { status: 404 },
  });
});

test("history without an executing assistant produces an explicit error", async () => {
  const { task, state } = await setup();
  state.messages = [user(), user("other", "queued")];
  await expect(task("astra")).rejects.toThrow("Unable to resolve executing assistant");
  state.messages = [];
  await expect(task("astra")).rejects.toThrow("Unable to resolve executing assistant");
});

test("completed and compaction assistants cannot authorize task execution", async () => {
  const { task, state } = await setup();
  for (const extra of [{ time: { created: 1, completed: 2 } }, { summary: true }]) {
    state.messages = [{ ...assistant(), info: { ...assistant().info, ...extra } }];
    await expect(task("astra")).rejects.toThrow("Unable to resolve executing assistant");
  }
});

test("command subtask with missing invoking user fails explicitly", async () => {
  const { task } = await setup([assistant("openai", [commandPart])]);
  await expect(task("astra", "part")).rejects.toMatchObject({
    message: "Missing /session/session/message/user", cause: { status: 404 },
  });
});

test("command subtask rejects an assistant in place of its invoking user", async () => {
  const parent = assistant();
  parent.info.id = "user";
  const { task } = await setup([parent, assistant("openai", [commandPart])]);
  await expect(task("astra", "part")).rejects.toThrow("Unable to resolve invoking user for command task part");
});
