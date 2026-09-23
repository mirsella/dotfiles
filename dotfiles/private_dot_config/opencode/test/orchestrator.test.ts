import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Config } from "@opencode-ai/plugin";
import type { AssistantMessage, Part, Session, SessionMessagesResponse, ToolPart, UserMessage } from "@opencode-ai/sdk";
import { createOpencodeClient } from "@opencode-ai/sdk";
import { MODE_MODELS, readState, writeState, type State } from "../lib/subagent-mode";
import orchestrator from "../plugins/orchestrator";

type Model = { providerID: string; modelID: string; variant?: string };
type Marker = { fast: boolean } | { model: Model };

const luna: Model = { providerID: "openai", modelID: "gpt-6-luna", variant: "max" };
const deepseek: Model = MODE_MODELS.go;
const solFast = (variant: "high" | "low"): Model => ({ providerID: "openai", modelID: "gpt-5.6-sol-fast", variant });
const astra: Model = { providerID: "openai", modelID: "gpt-6-astra" };
const astraFast: Model = { providerID: "openai", modelID: "gpt-6-astra-fast" };
const allWorkers = ["general", "explore", "astra"] as const;
const markerPattern = /^<opencode-orchestrator-[^:]+:(\{.*?\})>\n/;

const markerOf = (prompt: string): Marker => {
  const match = markerPattern.exec(prompt);
  if (match === null) throw new Error(`Missing orchestrator marker in ${JSON.stringify(prompt)}`);
  return JSON.parse(match[1]) as Marker;
};
const auto = (fast: boolean): Marker => ({ fast });
const forced = (model: Model): Marker => ({ model });
const inherits = (prompt: string) => expect(prompt).toBe(taskPrompt);

const user = (providerID = "openai", id = "user") => ({
  info: {
    id, sessionID: "session", role: "user", agent: "build", time: { created: 1 },
    model: { providerID, modelID: "gpt-6-astra" },
  } satisfies UserMessage,
  parts: [],
});
const fastUser = (providerID = "openai", id = "user") => {
  const message = user(providerID, id);
  message.info.model.modelID += "-fast";
  return message;
};
const assistant = (providerID = "openai", parts: Part[] = []) => ({
  info: {
    id: "assistant", sessionID: "session", parentID: "user", role: "assistant", providerID,
    modelID: "gpt-6-astra", mode: "build", time: { created: 1 },
    path: { cwd: "/project", root: "/project" }, cost: 0,
    tokens: { input: 0, output: 0, reasoning: 0, cache: { read: 0, write: 0 } },
  } satisfies AssistantMessage,
  parts,
});
const fastAssistant = (providerID = "openai", parts: Part[] = []) => {
  const message = assistant(providerID, parts);
  message.info.modelID += "-fast";
  return message;
};

const commandPart: ToolPart = {
  id: "part", sessionID: "session", messageID: "assistant", type: "tool", tool: "task",
  callID: "different-call", state: { status: "running", input: {}, time: { start: 1 } },
};

const taskPrompt = "Inspect @src/main.ts\nReport findings.\n";

const setup = async (messages: SessionMessagesResponse = [user(), assistant()]) => {
  const file = join(mkdtempSync(join(tmpdir(), "orchestrator-")), "subagent-mode.json");
  process.env.OPENCODE_SUBAGENT_MODE_FILE = file;
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
    return message.model as Model;
  };
  const definition = async (toolID = "task", description = "Original tool description") => {
    const output = { description, parameters: {} };
    await hooks["tool.definition"]({ toolID }, output);
    return output.description;
  };
  const system = async (providerID = "openai", modelID = "gpt-6-astra") => {
    const output = { system: [] as string[] };
    await hooks["experimental.chat.system.transform"](
      { model: { providerID, modelID } as Model },
      output,
    );
    return output.system;
  };
  const setState = (subagentState: State) => writeState(subagentState, file);
  return { hooks, state, histories, requests, task, prompt, definition, system, setState };
};

test("registers astra defaults and leaves other agents untouched", async () => {
  const general = { mode: "subagent" as const };
  const explore = { model: "openai/gpt-5.6-sol#high" };
  const config: Config = { model: "openai/gpt-6-astra", agent: { general, explore } };
  const { hooks } = await setup();
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.agent!.general).toBe(general);
  expect(config.agent!.explore).toBe(explore);
  expect(Object.keys(config.agent!)).toEqual(allWorkers);
  expect(config.agent!.astra).toMatchObject({
    mode: "subagent",
    model: "openai/gpt-6-astra",
    options: { reasoningEffort: "low" },
  });
  expect(config.agent!.astra.description).toBeString();
  expect(config.agent!.luna).toBeUndefined();
});

test("preserves explicit astra settings and stays idempotent", async () => {
  const custom = {
    permission: { edit: "deny" as const }, disable: true, prompt: "Read only.",
    variant: "custom", model: "openai/custom-astra", mode: "all" as const,
    description: "Custom worker", options: { reasoningEffort: "high", custom: true },
  };
  const config: Config = { agent: { astra: custom } };
  const { hooks } = await setup();
  await hooks.config(config);
  expect(config.agent!.astra).toEqual(custom);
  const registered = structuredClone(config);
  await hooks.config(config);
  expect(config).toEqual(registered);
});

test("task definition carries the delegation policy without service lookups", async () => {
  const { definition, requests } = await setup();
  const description = await definition();
  expect(description).toStartWith("Original tool description\n\nDelegation policy for task calls:");
  for (const text of [
    "general and explore are the default workers",
    "Choose astra yourself only in OpenAI main sessions",
    "call it only when the user explicitly requests it",
    "unavailable from child sessions",
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

test("system prompt allows astra only in OpenAI sessions", async () => {
  const { system } = await setup();
  expect(await system("openai")).toEqual([
    expect.stringContaining("you are in an OpenAI main session"),
  ]);
  expect((await system("openai"))[0]).toContain("choose astra yourself");
});

test("system prompt forbids astra for other providers unless explicitly named", async () => {
  const { system } = await setup();
  for (const providerID of ["openrouter", "opencode-go", "unknown"]) {
    const injected = await system(providerID);
    expect(injected).toHaveLength(1);
    expect(injected[0]).toContain(`you are in a ${providerID} main session, not OpenAI`);
    expect(injected[0]).toContain('NEVER delegate to astra unless the latest user message explicitly names "astra"');
    expect(injected[0]).toContain("unavailable from child sessions");
  }
});

test("OpenAI main sessions tag every worker with the current speed", async () => {
  const { task } = await setup();
  for (const agent of allWorkers) {
    expect(markerOf(await task(agent))).toEqual(auto(false));
  }
  const accelerated = await setup([user(), fastAssistant()]);
  for (const agent of allWorkers) {
    expect(markerOf(await accelerated.task(agent))).toEqual(auto(true));
  }
});

test("non-OpenAI main sessions pass prompts through untouched", async () => {
  const { task, state } = await setup();
  state.messages = [user("other"), assistant("other")];
  for (const agent of allWorkers) {
    expect(await task(agent)).toBe(taskPrompt);
  }
});

test("child sessions reject every worker before paging history", async () => {
  const { task, state, requests } = await setup([assistant("openai", [commandPart]), ...Array.from({ length: 5 }, (_, i) => user("openai", `queued-${i}`))]);
  state.session!.parentID = "parent";
  for (const agent of allWorkers) {
    await expect(task(agent, "part")).rejects.toThrow("available only from main sessions");
  }
  expect(requests).toEqual(["get", "get", "get"]);
});

test("unrelated tools and agent types need no lookups", async () => {
  const { task, requests } = await setup();
  expect(await task("custom")).toBe(taskPrompt);
  expect(await task("build", "call", "read")).toBe(taskPrompt);
  expect(requests).toEqual([]);
});

test("uses each executing assistant's provider rather than remembered or latest user models", async () => {
  const { task, state } = await setup();
  for (const agent of allWorkers) {
    expect(markerOf(await task(agent))).toEqual(auto(false));
  }
  state.messages = [user(), assistant("other"), user("openai", "queued")];
  for (const agent of allWorkers) inherits(await task(agent));
  state.messages = [user("other"), assistant(), user("other", "queued")];
  for (const agent of allWorkers) {
    expect(markerOf(await task(agent))).toEqual(auto(false));
  }
});

test("finds the executing assistant behind queued users and shares its model across parallel tasks", async () => {
  const { task, state, requests } = await setup();
  state.messages!.push(...Array.from({ length: 5 }, (_, i) => user("other", `queued-${i}`)));
  const prompts = await Promise.all([task("general", "call-1"), task("astra", "call-2"), task("explore", "call-3")]);
  for (const prompt of prompts) expect(markerOf(prompt)).toEqual(auto(false));
  expect(requests.sort()).toEqual([
    "get", "get", "get",
    "messages:2", "messages:2", "messages:2",
    "messages:4", "messages:4", "messages:4",
    "messages:8", "messages:8", "messages:8",
  ]);
});

test("command subtasks use their exact parent user's provider, not the target or queued model", async () => {
  const { task, state, requests } = await setup();
  state.messages = [user("other"), assistant("openai", [commandPart]), user("openai", "queued")];
  await expect(task("astra", "part")).resolves.toBe(taskPrompt);
  await expect(task("general", "part")).resolves.toBe(taskPrompt);
  state.messages = [user("openai"), assistant("other", [commandPart]), user("other", "queued")];
  expect(markerOf(await task("astra", "part"))).toEqual(auto(false));
  expect(markerOf(await task("general", "part"))).toEqual(auto(false));
  expect(requests.filter((request) => request.startsWith("message:"))).toEqual(Array(4).fill("message:user"));
});

test("command subtasks reuse an invoking user already in the fetched page", async () => {
  const { task, requests } = await setup([user(), assistant("other", [commandPart])]);
  expect(markerOf(await task("astra", "part"))).toEqual(auto(false));
  expect(requests).toEqual(["get", "messages:2"]);
});

test("command subtasks resolve the assistant's parent ID, not a fixed or latest user", async () => {
  const invoking = fastUser("openai", "invoking-user");
  const command = assistant("other", [commandPart]);
  command.info.parentID = invoking.info.id;
  const { task, requests } = await setup([invoking, command, user("other", "queued")]);
  expect(markerOf(await task("astra", "part"))).toEqual(auto(true));
  expect(requests).toEqual(["get", "messages:2", "message:invoking-user"]);
});

test("normal task call IDs do not select the command authorization path", async () => {
  const { task, requests } = await setup([user("other"), assistant("openai", [commandPart])]);
  expect(markerOf(await task("astra", commandPart.callID))).toEqual(auto(false));
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

test("OpenAI normal sessions run general and explore on luna max", async () => {
  const { task, prompt } = await setup();
  for (const agent of ["general", "explore"]) {
    expect(await prompt("child", agent, "gpt-6-astra", await task(agent))).toMatchObject(luna);
  }
});

test("OpenAI fast sessions run general high and explore low on sol-fast", async () => {
  const { task, prompt } = await setup([user(), fastAssistant()]);
  for (const [agent, variant] of [["general", "high"], ["explore", "low"]] as const) {
    expect(await prompt("child", agent, "gpt-6-astra", await task(agent))).toMatchObject(solFast(variant));
  }
});

test("non-OpenAI main sessions leave worker models to inheritance", async () => {
  const { task, prompt, state } = await setup();
  state.messages = [user("other"), assistant("other")];
  expect(await task("general")).toBe(taskPrompt);
  expect((await prompt("child", "general", "deepseek-v4.1-flash", taskPrompt, "opencode-go")).variant).toBe("custom");
  expect(await task("astra")).toBe(taskPrompt);
  expect((await prompt("child", "astra", "gpt-6-astra")).modelID).toBe("gpt-6-astra");
});

test("forced Go mode rewrites general and explore from any provider", async () => {
  const { task, prompt, state, setState } = await setup();
  setState({ global: { mode: "go" } });
  for (const provider of ["openai", "other"]) {
    state.messages = [user(provider), assistant(provider)];
    for (const agent of ["general", "explore"]) {
      const marked = await task(agent);
      expect(markerOf(marked)).toEqual(forced(deepseek));
      expect(await prompt("child", agent, "gpt-6-luna", marked)).toMatchObject(deepseek);
    }
  }
});

test("forced Codex mode pins workers to luna max regardless of speed", async () => {
  const { task, prompt, state, setState } = await setup();
  setState({ global: { mode: "codex" } });
  for (const fast of [false, true]) {
    state.messages = [fast ? fastUser() : user(), fast ? fastAssistant() : assistant()];
    for (const agent of ["general", "explore"]) {
      const marked = await task(agent);
      expect(markerOf(marked)).toEqual(forced(luna));
      expect(await prompt("child", agent, "deepseek-v4.1-flash", marked, "opencode-go")).toMatchObject(luna);
    }
  }
});

test("astra keeps its OpenAI family when workers are forced elsewhere", async () => {
  const { task, prompt, state, setState } = await setup();
  setState({ global: { mode: "go" } });
  const marked = await task("astra");
  expect(markerOf(marked)).toEqual(auto(false));
  expect(await prompt("child", "astra", "gpt-6-astra", marked)).toMatchObject(astra);
  state.messages = [user("other"), assistant("other")];
  inherits(await task("astra"));
});

test("session scopes override the global mode and model", async () => {
  const { task, prompt, histories, state, setState } = await setup();
  const custom = { providerID: "opencode-go", modelID: "kimi-k3" };
  setState({
    global: { mode: "go", models: { codex: luna } },
    sessions: { session: { mode: "codex", models: { codex: custom } } },
  });
  histories.other = {
    session: { ...state.session!, id: "other" },
    messages: [user("other"), assistant("other")],
  };
  const expectations = [
    { sessionID: "session", ...custom },
    { sessionID: "other", ...deepseek },
  ];
  for (const { sessionID, ...spec } of expectations) {
    const marked = await task("general", "call", "task", sessionID);
    expect(markerOf(marked)).toEqual(forced(spec));
    expect(await prompt(`child-${sessionID}`, "general", "gpt-6-astra", marked)).toMatchObject(spec);
  }
});

test("custom models persist across server restarts", async () => {
  const { hooks, setState } = await setup();
  const state: State = {
    global: { mode: "go", models: { go: { providerID: "opencode-go", modelID: "kimi-k3" } } },
    sessions: { one: { mode: "codex", models: { codex: luna } } },
  };
  setState(state);
  await hooks.config({});
  expect(readState(process.env.OPENCODE_SUBAGENT_MODE_FILE!)).toEqual(state);
});

test("deleted sessions drop their overrides", async () => {
  const { hooks, setState } = await setup();
  setState({ global: { mode: "go" }, sessions: { session: { mode: "codex" }, other: {} } });
  const file = process.env.OPENCODE_SUBAGENT_MODE_FILE!;
  const deleted = (id: string) =>
    hooks.event({ event: { type: "session.deleted", properties: { info: { id } } } });
  await deleted("session");
  expect(readState(file)).toEqual({ global: { mode: "go" }, sessions: { other: {} } });
  await deleted("missing");
  expect(readState(file)).toEqual({ global: { mode: "go" }, sessions: { other: {} } });
});

test("unreadable state files fall back to auto", async () => {
  const { task } = await setup();
  writeFileSync(process.env.OPENCODE_SUBAGENT_MODE_FILE!, "{not json");
  expect(markerOf(await task("general"))).toEqual(auto(false));
});

test("astra follows fast mode and keeps explicit custom models authoritative", async () => {
  const { task, prompt, state } = await setup();
  const normal = await task("astra");
  const normalModel = await prompt("child", "astra", "gpt-6-astra", normal);
  expect(normalModel).toMatchObject(astra);
  // Fixed workers keep the variant they arrived with.
  expect(normalModel.variant).toBe("custom");
  state.messages = [user(), fastAssistant()];
  const accelerated = await task("astra");
  const fastModel = await prompt("child", "astra", "gpt-6-astra", accelerated);
  expect(fastModel).toMatchObject(astraFast);
  expect(fastModel.variant).toBe("custom");
  expect(await prompt("child", "astra", "gpt-6-astra-fast", accelerated)).toMatchObject(astraFast);
  expect(await prompt("child", "astra", "custom-astra-fast", accelerated)).toMatchObject({ modelID: "custom-astra-fast" });
  expect(await prompt("child", "astra", "gpt-6-astra-fast", normal)).toMatchObject(astra);
});

test("unrelated prompts and non-worker agents are unchanged", async () => {
  const { task, prompt } = await setup();
  expect((await prompt("session", "build", "gpt-6-astra-fast")).modelID).toBe("gpt-6-astra-fast");
  expect((await prompt("child", "luna", "gpt-6-luna", await task("astra"))).modelID).toBe("gpt-6-luna");
  expect((await prompt("child", "astra", "gpt-6-astra-fast", await task("astra"), "other")).modelID).toBe("gpt-6-astra-fast");
});

test("all workers follow live fast/non-fast switches, including resumed child sessions", async () => {
  const { state, task, prompt } = await setup();
  const expected = {
    general: { normal: luna, fast: solFast("high") },
    explore: { normal: luna, fast: solFast("low") },
    astra: { normal: astra, fast: astraFast },
  } as const;
  for (const fast of [false, true, false]) {
    const queued = user("openai", "queued");
    queued.info.model.modelID = fast ? "gpt-6-astra" : "gpt-6-astra-fast";
    state.messages = [user(), fast ? fastAssistant() : assistant(), queued];
    for (const agent of allWorkers) {
      expect(await prompt("child", agent, "gpt-6-astra", await task(agent))).toMatchObject(expected[agent][fast ? "fast" : "normal"]);
    }
  }
});

test("snapshots the invoking model rather than a later turn and correlates parallel children", async () => {
  const { state, task, prompt } = await setup([user(), fastAssistant()]);
  const [fastAstra, fastGeneral] = await Promise.all([task("astra", "fast-astra"), task("general", "fast-general")]);
  state.messages = [user(), assistant()];
  const normalAstra = await task("astra", "normal-astra");
  expect(await prompt("fast-astra", "astra", "gpt-6-astra", fastAstra)).toMatchObject(astraFast);
  expect(await prompt("normal", "astra", "gpt-6-astra-fast", normalAstra)).toMatchObject(astra);
  expect(await prompt("fast-general", "general", "gpt-6-astra", fastGeneral)).toMatchObject(solFast("high"));
});

test("concurrent main sessions with identical call IDs keep independent speed settings", async () => {
  const { state, histories, task, prompt } = await setup();
  histories.other = { session: { ...state.session!, id: "other" }, messages: [user(), fastAssistant()] };
  const [normal, accelerated] = await Promise.all([task("astra"), task("astra", "call", "task", "other")]);
  expect(await prompt("normal", "astra", "gpt-6-astra-fast", normal)).toMatchObject(astra);
  expect(await prompt("fast", "astra", "gpt-6-astra", accelerated)).toMatchObject(astraFast);
});

test("command tasks inherit the invoking user's speed and correlate by part ID", async () => {
  const { task, prompt } = await setup([fastUser(), assistant("other", [commandPart]), user("openai", "queued")]);
  expect(await prompt("child", "astra", "gpt-6-astra", await task("astra", "part"))).toMatchObject(astraFast);
});

test("queued resumes carry their own speed regardless of scheduling order or cancellation", async () => {
  const { state, task, prompt } = await setup();
  const normal = await task("astra", "normal");
  await task("astra", "cancelled");
  state.messages = [user(), fastAssistant()];
  const accelerated = await task("astra", "fast");
  // The cancelled invocation never reaches chat.message; no cleanup event is needed.
  expect(await prompt("child", "astra", "gpt-6-astra", accelerated)).toMatchObject(astraFast);
  expect(await prompt("child", "astra", "gpt-6-astra-fast", normal)).toMatchObject(astra);
  expect(await prompt("child", "astra", "gpt-6-astra-fast")).toMatchObject(astraFast);
});

test("later hook corrections keep the parent's speed without pinning the original agent", async () => {
  const { state, task, prompt } = await setup();
  for (const fast of [false, true]) {
    state.messages = [user(), fast ? fastAssistant() : assistant()];
    const source = await task("astra");
    // The watchdog rewrites subagent_type when resuming a different original worker.
    expect(await prompt("child", "general", "gpt-6-luna", source)).toMatchObject(fast ? solFast("high") : luna);
    expect(await prompt("child", "explore", "gpt-6-luna", source)).toMatchObject(fast ? solFast("low") : luna);
  }
});

test("corrupted model markers fail explicitly", async () => {
  const { task, prompt } = await setup();
  const source = await task("astra");
  const prefix = source.slice(0, source.indexOf("{"));
  const corrupted = [
    `${prefix}{not json}>\n${taskPrompt}`,
    `${prefix}{"fast":"yes"}>\n${taskPrompt}`,
    `${prefix}{"fast":false,"model":{"providerID":"openai","modelID":"gpt-6-luna"}}>\n${taskPrompt}`,
    `${prefix}{"model":{"providerID":"openai"}}>\n${taskPrompt}`,
    `${prefix}{}>\n${taskPrompt}`,
    `${prefix}{"fast":false}${taskPrompt}`,
  ];
  for (const text of corrupted) {
    await expect(prompt("child", "astra", "gpt-6-astra", text)).rejects.toThrow("Invalid orchestrator model marker");
  }
});

test.each(["", "\n@src/main.ts\n\u00e9\r\n", "<opencode-orchestrator-user:sol:fast>\nKeep this text."])(
  "removes only routing metadata, preserving prompt %j and attachments",
  async (text) => {
    const { hooks } = await setup();
    for (const agent of allWorkers) {
      const args = { subagent_type: agent, prompt: text };
      await hooks["tool.execute.before"]({ tool: "task", sessionID: "session", callID: "call" }, { args });
      const message = { ...user().info, sessionID: "child", agent, model: { providerID: "openai", modelID: "gpt-6-astra-fast" } };
      const parts: Part[] = [
        { id: "file", sessionID: "child", messageID: message.id, type: "file", mime: "text/plain", url: "file:///project/src/main.ts" },
        { id: "text", sessionID: "child", messageID: message.id, type: "text", text },
      ];
      const expected = structuredClone(parts);
      parts[1] = { ...parts[1], type: "text", text: args.prompt };
      await hooks["chat.message"]({ sessionID: "child", agent }, { message, parts });
      expect(parts).toEqual(expected);
      await hooks["chat.message"]({ sessionID: "child", agent }, { message, parts });
      expect(parts).toEqual(expected);
    }
  },
);
