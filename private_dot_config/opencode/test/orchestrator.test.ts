import { expect, test } from "bun:test";
import type { Config } from "@opencode-ai/plugin";
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
      const [, , sessionID, resource, messageID] = url.pathname.split("/");
      const source = histories[sessionID];
      let data;
      switch (resource) {
        case undefined:
          requests.push("get");
          data = source?.session;
          break;
        case "message": {
          if (messageID) {
            requests.push(`message:${messageID}`);
            data = source?.messages?.find(({ info }) => info.id === messageID);
            break;
          }
          const limit = Number(url.searchParams.get("limit"));
          requests.push(`messages:${limit}`);
          data = source?.messages?.slice(-limit);
          break;
        }
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
  expect(Object.keys(config.agent!)).toEqual(["general", "luna", "astra"]);
  for (const [name, model, effort] of [
    ["luna", "openai/gpt-5.6-luna", "max"],
    ["astra", "openai/gpt-6-astra", "low"],
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
  const config: Config = { agent: { luna, astra: { options: { custom: true } } } };
  const { hooks } = await setup();
  await hooks.config(config);
  expect(config.agent!.luna).toEqual(luna);
  expect(config.agent!.astra.options).toEqual({ reasoningEffort: "low", custom: true });
  const registered = structuredClone(config);
  await hooks.config(config);
  expect(config).toEqual(registered);
});

test("task definition carries the conditional delegation policy without service lookups", async () => {
  const { definition, requests } = await setup();
  const description = await definition();
  expect(description).toStartWith("Original tool description\n\nDelegation policy for task calls:");
  for (const text of [
    "When delegating from an OpenAI main session",
    "luna for routine, well-scoped subtasks and general otherwise",
    "named workers are unavailable from other providers or child sessions",
    "Other agents are unaffected",
    "only when the user explicitly requests orchestration for the current task",
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

test("cold OpenAI continuation allows workers and general before tool parts exist", async () => {
  const { task } = await setup();
  for (const agent of ["luna", "astra"]) await expect(task(agent)).resolves.toEndWith(taskPrompt);
  await expect(task("general")).resolves.toBe(taskPrompt);
});

test("uses each executing assistant's provider rather than remembered or latest user models", async () => {
  const { task, state } = await setup();
  await expect(task("astra")).resolves.toEndWith(taskPrompt);
  state.messages = [user(), assistant("other"), user("openai", "queued")];
  for (const agent of ["luna", "astra"]) {
    await expect(task(agent)).rejects.toThrow("available only from OpenAI main sessions");
  }
  await expect(task("general")).resolves.toBe(taskPrompt);
  state.messages = [user("other"), assistant(), user("other", "queued")];
  await expect(task("astra")).resolves.toEndWith(taskPrompt);
  await expect(task("general")).resolves.toBe(taskPrompt);
});

test("child sessions reject workers without paging history or resolving command parents", async () => {
  const { task, state, requests } = await setup([assistant("openai", [commandPart]), ...Array.from({ length: 5 }, (_, i) => user("openai", `queued-${i}`))]);
  state.session!.parentID = "parent";
  for (const agent of ["luna", "astra"]) {
    await expect(task(agent, "part")).rejects.toThrow("available only from OpenAI main sessions");
  }
  await expect(task("general")).resolves.toBe(taskPrompt);
  expect(requests.sort()).toEqual(["get", "get", "messages:2", "messages:2"]);
});

test.each([false, true])("general bypasses routing with child=%s, even without readable history", async (child) => {
  const { hooks, task, prompt, state, requests } = await setup();
  state.session!.parentID = child ? "parent" : undefined;
  state.messages = undefined;
  const config: Config = {};
  await hooks.config(config);
  expect(config.agent!.general).toBeUndefined();
  const source = await task("general");
  expect(source).toBe(taskPrompt);
  expect(await prompt("child", "general", "custom-parent-fast", source, "other")).toEqual({
    providerID: "other", modelID: "custom-parent-fast", variant: "custom",
  });
  expect(requests).toEqual([]);
});

test("unrelated tools and agent types need no lookups", async () => {
  const { task, requests } = await setup();
  expect(await task("explore")).toBe(taskPrompt);
  expect(await task("custom")).toBe(taskPrompt);
  expect(await task("general")).toBe(taskPrompt);
  expect(await task("astra", "call", "read")).toBe(taskPrompt);
  expect(requests).toEqual([]);
});

test("finds the executing assistant behind queued users and shares its model across parallel tasks", async () => {
  const { task, state, requests } = await setup();
  state.messages!.push(...Array.from({ length: 5 }, (_, i) => user("other", `queued-${i}`)));
  await Promise.all([task("luna", "call-1"), task("astra", "call-2")]);
  expect(requests.sort()).toEqual(["get", "get", "messages:2", "messages:2", "messages:4", "messages:4", "messages:8", "messages:8"]);
});

test("command subtasks use their exact parent user's provider, not the target or queued model", async () => {
  const { task, state, requests } = await setup();
  state.messages = [user("other"), assistant("openai", [commandPart]), user("openai", "queued")];
  await expect(task("astra", "part")).rejects.toThrow("available only from OpenAI main sessions");
  await expect(task("general", "part")).resolves.toBe(taskPrompt);
  state.messages = [user("openai"), assistant("other", [commandPart]), user("other", "queued")];
  await expect(task("astra", "part")).resolves.toEndWith(taskPrompt);
  await expect(task("general", "part")).resolves.toBe(taskPrompt);
  expect(requests.filter((request) => request.startsWith("message:"))).toEqual(Array(2).fill("message:user"));
});

test("command subtasks reuse an invoking user already in the fetched page", async () => {
  const { task, requests } = await setup([user(), assistant("other", [commandPart])]);
  await expect(task("astra", "part")).resolves.toEndWith(taskPrompt);
  expect(requests).toEqual(["get", "messages:2"]);
});

test("command subtasks resolve the assistant's parent ID, not a fixed or latest user", async () => {
  const invoking = user("openai", "invoking-user");
  invoking.info.model.modelID += "-fast";
  const command = assistant("other", [commandPart]);
  command.info.parentID = invoking.info.id;
  const { task, prompt, requests } = await setup([invoking, command, user("other", "queued")]);
  expect((await prompt("child", "astra", "gpt-6-astra", await task("astra", "part"))).modelID).toBe("gpt-6-astra-fast");
  expect(requests).toEqual(["get", "messages:2", "message:invoking-user"]);
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
  await expect(task("astra")).rejects.toMatchObject({
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
    for (const [agent, base] of [["luna", "gpt-5.6-luna"], ["astra", "gpt-6-astra"]]) {
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
  const [fastAstra, fastLuna] = await Promise.all([task("astra", "fast-astra"), task("luna", "fast-luna")]);
  state.messages = [user(), assistant()];
  const normalAstra = await task("astra", "normal-astra");
  expect((await prompt("fast-astra", "astra", "gpt-6-astra", fastAstra)).modelID).toBe("gpt-6-astra-fast");
  expect((await prompt("normal", "astra", "gpt-6-astra-fast", normalAstra)).modelID).toBe("gpt-6-astra");
  expect((await prompt("fast-luna", "luna", "gpt-5.6-luna", fastLuna)).modelID).toBe("gpt-5.6-luna-fast");
});

test("concurrent main sessions with identical call IDs keep independent speed settings", async () => {
  const { state, histories, task, prompt } = await setup();
  const fast = assistant();
  fast.info.modelID += "-fast";
  histories.other = { session: { ...state.session!, id: "other" }, messages: [user(), fast] };
  const [normal, accelerated] = await Promise.all([task("astra"), task("astra", "call", "task", "other")]);
  expect((await prompt("normal", "astra", "gpt-6-astra-fast", normal)).modelID).toBe("gpt-6-astra");
  expect((await prompt("fast", "astra", "gpt-6-astra", accelerated)).modelID).toBe("gpt-6-astra-fast");
});

test("command tasks inherit the invoking user's speed and correlate by part ID", async () => {
  const invoking = user();
  invoking.info.model.modelID += "-fast";
  const { task, prompt } = await setup([invoking, assistant("other", [commandPart]), user("openai", "queued")]);
  expect((await prompt("child", "astra", "gpt-6-astra", await task("astra", "part"))).modelID).toBe("gpt-6-astra-fast");
});

test("queued resumes carry their own speed regardless of scheduling order or cancellation", async () => {
  const { state, task, prompt } = await setup();
  const normal = await task("astra", "normal");
  await task("astra", "cancelled");
  const fast = assistant();
  fast.info.modelID += "-fast";
  state.messages = [user(), fast];
  const accelerated = await task("astra", "fast");
  // The cancelled invocation never reaches chat.message; no cleanup event is needed.
  expect((await prompt("child", "astra", "gpt-6-astra", accelerated)).modelID).toBe("gpt-6-astra-fast");
  expect((await prompt("child", "astra", "gpt-6-astra-fast", normal)).modelID).toBe("gpt-6-astra");
  expect((await prompt("child", "astra", "gpt-6-astra-fast")).modelID).toBe("gpt-6-astra-fast");
});

test("unrelated prompts and explicit custom worker models are unchanged", async () => {
  const { task, prompt, requests } = await setup();
  expect((await prompt("session", "build", "gpt-6-astra-fast")).modelID).toBe("gpt-6-astra-fast");
  expect((await prompt("child", "astra", "gpt-6-astra-fast")).modelID).toBe("gpt-6-astra-fast");
  expect(requests).toEqual([]);
  for (const [provider, model] of [["openai", "custom-astra-fast"], ["other", "gpt-6-astra-fast"]]) {
    expect((await prompt("child", "astra", model, await task("astra"), provider)).modelID).toBe(model);
  }
});

test("worker corrections by later hooks retain the parent's speed without pinning the original agent", async () => {
  const { state, task, prompt } = await setup();
  for (const fast of [false, true]) {
    const current = assistant();
    current.info.modelID = `gpt-6-astra${fast ? "-fast" : ""}`;
    state.messages = [user(), current];
    const source = await task("astra");
    // The watchdog rewrites subagent_type when resuming a different original worker.
    expect((await prompt("child", "luna", "gpt-5.6-luna", source)).modelID)
      .toBe(`gpt-5.6-luna${fast ? "-fast" : ""}`);
    expect((await prompt("child", "explore", "gpt-5.6-luna", source)).modelID).toBe("gpt-5.6-luna");
  }
});

test("corrupted model markers fail explicitly", async () => {
  const { task, prompt } = await setup();
  const source = await task("astra");
  await expect(prompt("child", "astra", "gpt-6-astra", source.replace(":normal>", ":invalid>"))).rejects.toThrow("Invalid orchestrator model marker");
});

test.each(["", "\n@src/main.ts\n\u00e9\r\n", "<opencode-orchestrator-user:sol:fast>\nKeep this text."])(
  "removes only routing metadata, preserving prompt %j and attachments",
  async (text) => {
    const { hooks } = await setup();
    const args = { subagent_type: "astra", prompt: text };
    await hooks["tool.execute.before"]({ tool: "task", sessionID: "session", callID: "call" }, { args });
    const message = { ...user().info, sessionID: "child", agent: "astra", model: { providerID: "openai", modelID: "gpt-6-astra-fast" } };
    const parts: Part[] = [
      { id: "file", sessionID: "child", messageID: message.id, type: "file", mime: "text/plain", url: "file:///project/src/main.ts" },
      { id: "text", sessionID: "child", messageID: message.id, type: "text", text },
    ];
    const expected = structuredClone(parts);
    parts[1] = { ...parts[1], type: "text", text: args.prompt };
    await hooks["chat.message"]({ sessionID: "child", agent: "astra" }, { message, parts });
    expect(parts).toEqual(expected);
    expect(message.model.modelID).toBe("gpt-6-astra");
    await hooks["chat.message"]({ sessionID: "child", agent: "astra" }, { message, parts });
    expect(parts).toEqual(expected);
  },
);
