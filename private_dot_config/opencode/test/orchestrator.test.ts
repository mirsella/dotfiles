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

const taskPrompt = "Inspect @src/main.ts\nReport findings.\n";

const setup = async (messages: SessionMessagesResponse = [user(), assistant()]) => {
  const session: Session = {
    id: "session", projectID: "project", directory: "/project", title: "Test",
    version: "1", time: { created: 1, updated: 1 },
  };
  type History = { messages?: SessionMessagesResponse; session?: Session };
  const state: History = { messages, session };
  const histories: Record<string, History> = { session: state };
  const requests: string[] = [];
  const client = createOpencodeClient({
    baseUrl: "http://opencode.test",
    fetch: async (request) => {
      const url = new URL(request.url);
      const sessionID = url.pathname.split("/")[2];
      const source = histories[sessionID];
      let data;
      switch (url.pathname.slice(`/session/${sessionID}`.length)) {
        case "":
          requests.push("get");
          data = source?.session;
          break;
        case "/message": {
          const limit = Number(url.searchParams.get("limit"));
          requests.push(`messages:${limit}`);
          data = source?.messages?.slice(-limit);
          break;
        }
        case "/message/user":
          requests.push("message:user");
          data = source?.messages?.find(({ info }) => info.id === "user");
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
  const task = async (subagent_type: string, callID = "call", tool = "task", sessionID = "session") => {
    const args = { subagent_type, prompt: taskPrompt };
    await hooks["tool.execute.before"]({ tool, sessionID, callID }, { args });
    return args.prompt;
  };
  const prompt = async (sessionID: string, agent: string, modelID: string, source = taskPrompt, providerID = "openai") => {
    const message = { ...user(providerID).info, sessionID, agent, model: { providerID, modelID, variant: "custom" } };
    const parts: Part[] = [{ id: "text", sessionID, messageID: message.id, type: "text", text: source }];
    await hooks["chat.message"]({ sessionID, agent }, { message, parts });
    expect(parts).toEqual([{ id: "text", sessionID, messageID: message.id, type: "text", text: taskPrompt }]);
    return message.model;
  };
  const definition = async (toolID = "task", description = "Original tool description") => {
    const output = { description, parameters: {} };
    await hooks["tool.definition"]({ toolID }, output);
    return output.description;
  };
  return { hooks, state, histories, requests, task, prompt, definition };
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

test("task definition carries the conditional delegation policy without service lookups", async () => {
  const { definition, requests } = await setup();
  const description = await definition();
  expect(description).toStartWith("Original tool description\n\nDelegation policy for task calls:");
  for (const text of [
    "top-level session running an OpenAI model",
    "Do not use general", "legitimately available alternative",
    "most delegated work", "fresh perspective", "escalation does not require a separate user request",
    "defaults, not quotas", "non-overlapping writes", "reuse existing task IDs",
    "deliberate comparison and verification", "explicitly requests orchestration",
    "understand the implementation", "assess tradeoffs", "inspect key code",
    "including non-OpenAI and child sessions", "use general for general-purpose subtasks",
    "luna, sol, and astra workers are unavailable",
  ]) {
    expect(description).toContain(text);
  }
  expect(requests).toEqual([]);
});

test("leaves unrelated tool definitions unchanged", async () => {
  const { definition, requests } = await setup();
  expect(await definition("read", "Read files")).toBe("Read files");
  expect(requests).toEqual([]);
});

test("does not install LLM preparation hooks", async () => {
  const { hooks } = await setup();
  for (const name of ["experimental.chat.system.transform", "experimental.chat.messages.transform", "chat.params"] as const) {
    expect((hooks as Hooks)[name]).toBeUndefined();
  }
});

test("cold OpenAI continuation allows workers and denies general before tool parts exist", async () => {
  const { task } = await setup();
  for (const agent of ["luna", "sol", "astra"]) await expect(task(agent)).resolves.toEndWith(taskPrompt);
  await expect(task("general")).rejects.toThrow("general is unavailable from OpenAI main sessions");
});

test("uses each executing assistant's provider rather than remembered or latest user models", async () => {
  const { task, state } = await setup();
  await expect(task("astra")).resolves.toEndWith(taskPrompt);
  state.messages = [user(), assistant("other"), user("openai", "queued")];
  for (const agent of ["luna", "sol", "astra"]) {
    await expect(task(agent)).rejects.toThrow("available only from OpenAI main sessions");
  }
  await expect(task("general")).resolves.toBe(taskPrompt);
  state.messages = [user("other"), assistant(), user("other", "queued")];
  await expect(task("astra")).resolves.toEndWith(taskPrompt);
  await expect(task("general")).rejects.toThrow("unavailable from OpenAI main sessions");
});

test("child sessions cannot invoke workers but can use general", async () => {
  const { task, state } = await setup();
  state.session!.parentID = "parent";
  for (const agent of ["luna", "sol", "astra"]) {
    await expect(task(agent)).rejects.toThrow("available only from OpenAI main sessions");
  }
  await expect(task("general")).resolves.toBe(taskPrompt);
});

test("unrelated tools and agent types need no lookups", async () => {
  const { task, requests } = await setup();
  expect(await task("explore")).toBe(taskPrompt);
  expect(await task("custom")).toBe(taskPrompt);
  expect(await task("astra", "call", "read")).toBe(taskPrompt);
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
  await expect(task("general", "part")).resolves.toBe(taskPrompt);
  state.messages = [user("openai"), assistant("other", [commandPart]), user("other", "queued")];
  await expect(task("astra", "part")).resolves.toEndWith(taskPrompt);
  await expect(task("general", "part")).rejects.toThrow("unavailable from OpenAI main sessions");
  expect(requests.filter((request) => request.startsWith("message:"))).toEqual(Array(4).fill("message:user"));
});

test("command subtasks reuse an invoking user already in the fetched page", async () => {
  const { task, requests } = await setup([user(), assistant("other", [commandPart])]);
  await expect(task("astra", "part")).resolves.toEndWith(taskPrompt);
  expect(requests).toEqual(["get", "messages:2"]);
});

test("normal task call IDs do not select the command authorization path", async () => {
  const { task, requests } = await setup([user("other"), assistant("openai", [commandPart])]);
  await expect(task("astra", commandPart.callID)).resolves.toEndWith(taskPrompt);
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

test("all workers follow live fast/non-fast switches, including resumed child sessions", async () => {
  const { state, task, prompt } = await setup();
  for (const fast of [false, true, false]) {
    const current = assistant();
    current.info.modelID = `gpt-6-astra${fast ? "-fast" : ""}`;
    const queued = user("openai", "queued");
    queued.info.model.modelID = fast ? "gpt-6-astra" : "gpt-6-astra-fast";
    state.messages = [user(), current, queued];
    for (const [agent, base] of [["luna", "gpt-5.6-luna"], ["sol", "gpt-5.6-sol"], ["astra", "gpt-6-astra"]]) {
      const source = await task(agent);
      for (const configured of [base, `${base}-fast`]) {
        expect(await prompt(agent, agent, configured, source)).toEqual({
          providerID: "openai", modelID: `${base}${fast ? "-fast" : ""}`, variant: "custom",
        });
      }
    }
  }
});

test("snapshots the invoking model rather than a later turn and correlates parallel children", async () => {
  const { state, task, prompt } = await setup();
  const fast = assistant();
  fast.info.modelID += "-fast";
  state.messages = [user(), fast];
  const [fastSol, fastLuna] = await Promise.all([task("sol", "fast-sol"), task("luna", "fast-luna")]);
  state.messages = [user(), assistant()];
  const normalSol = await task("sol", "normal-sol");
  expect((await prompt("fast-sol", "sol", "gpt-5.6-sol", fastSol)).modelID).toBe("gpt-5.6-sol-fast");
  expect((await prompt("normal", "sol", "gpt-5.6-sol-fast", normalSol)).modelID).toBe("gpt-5.6-sol");
  expect((await prompt("fast-luna", "luna", "gpt-5.6-luna", fastLuna)).modelID).toBe("gpt-5.6-luna-fast");
});

test("concurrent main sessions with identical call IDs keep independent speed settings", async () => {
  const { state, histories, task, prompt } = await setup();
  const fast = assistant();
  fast.info.modelID += "-fast";
  histories.other = { session: { ...state.session!, id: "other" }, messages: [user(), fast] };
  const [normal, accelerated] = await Promise.all([task("sol"), task("sol", "call", "task", "other")]);
  expect((await prompt("normal", "sol", "gpt-5.6-sol-fast", normal)).modelID).toBe("gpt-5.6-sol");
  expect((await prompt("fast", "sol", "gpt-5.6-sol", accelerated)).modelID).toBe("gpt-5.6-sol-fast");
});

test("command tasks inherit the invoking user's speed and correlate by part ID", async () => {
  const invoking = user();
  invoking.info.model.modelID += "-fast";
  const { task, prompt } = await setup([invoking, assistant("other", [commandPart]), user("openai", "queued")]);
  expect((await prompt("child", "astra", "gpt-6-astra", await task("astra", "part"))).modelID).toBe("gpt-6-astra-fast");
});

test("queued resumes carry their own speed regardless of scheduling order or cancellation", async () => {
  const { state, task, prompt } = await setup();
  const normal = await task("sol", "normal");
  await task("sol", "cancelled");
  const fast = assistant();
  fast.info.modelID += "-fast";
  state.messages = [user(), fast];
  const accelerated = await task("sol", "fast");
  // The cancelled invocation never reaches chat.message; no cleanup event is needed.
  expect((await prompt("child", "sol", "gpt-5.6-sol", accelerated)).modelID).toBe("gpt-5.6-sol-fast");
  expect((await prompt("child", "sol", "gpt-5.6-sol-fast", normal)).modelID).toBe("gpt-5.6-sol");
  expect((await prompt("child", "sol", "gpt-5.6-sol-fast")).modelID).toBe("gpt-5.6-sol-fast");
});

test("unrelated prompts and explicit custom worker models are unchanged", async () => {
  const { task, prompt, requests } = await setup();
  expect((await prompt("session", "build", "gpt-6-astra-fast")).modelID).toBe("gpt-6-astra-fast");
  expect((await prompt("child", "sol", "gpt-5.6-sol-fast")).modelID).toBe("gpt-5.6-sol-fast");
  expect(requests).toEqual([]);
  for (const [provider, model] of [["openai", "custom-sol-fast"], ["other", "gpt-5.6-sol-fast"]]) {
    expect((await prompt("child", "sol", model, await task("sol"), provider)).modelID).toBe(model);
  }
});

test("worker corrections by later hooks retain the parent's speed without pinning the original agent", async () => {
  const { state, task, prompt } = await setup();
  for (const fast of [false, true]) {
    const current = assistant();
    current.info.modelID = `gpt-6-astra${fast ? "-fast" : ""}`;
    state.messages = [user(), current];
    const source = await task("sol");
    // The watchdog rewrites subagent_type when resuming a different original worker.
    expect((await prompt("child", "luna", "gpt-5.6-luna", source)).modelID)
      .toBe(`gpt-5.6-luna${fast ? "-fast" : ""}`);
    expect((await prompt("child", "explore", "gpt-5.6-luna", source)).modelID).toBe("gpt-5.6-luna");
  }
});

test("corrupted model markers fail explicitly", async () => {
  const { task, prompt } = await setup();
  const source = await task("sol");
  await expect(prompt("child", "sol", "gpt-5.6-sol", source.replace(":normal>", ":invalid>"))).rejects.toThrow("Invalid orchestrator model marker");
});

test.each(["", "\n@src/main.ts\n\u00e9\r\n", "<opencode-orchestrator-user:sol:fast>\nKeep this text."])(
  "removes only routing metadata, preserving prompt %j and attachments",
  async (text) => {
    const { hooks } = await setup();
    const args = { subagent_type: "sol", prompt: text };
    await hooks["tool.execute.before"]({ tool: "task", sessionID: "session", callID: "call" }, { args });
    const message = { ...user().info, sessionID: "child", agent: "sol", model: { providerID: "openai", modelID: "gpt-5.6-sol-fast" } };
    const parts: Part[] = [
      { id: "file", sessionID: "child", messageID: message.id, type: "file", mime: "text/plain", url: "file:///project/src/main.ts" },
      { id: "text", sessionID: "child", messageID: message.id, type: "text", text },
    ];
    const expected = structuredClone(parts);
    parts[1] = { ...parts[1], type: "text", text: args.prompt };
    await hooks["chat.message"]({ sessionID: "child", agent: "sol" }, { message, parts });
    expect(parts).toEqual(expected);
    expect(message.model.modelID).toBe("gpt-5.6-sol");
    await hooks["chat.message"]({ sessionID: "child", agent: "sol" }, { message, parts });
    expect(parts).toEqual(expected);
  },
);
