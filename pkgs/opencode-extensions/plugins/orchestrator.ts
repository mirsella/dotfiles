import type { Plugin } from "@opencode-ai/plugin";
import { asModelSpec, formatModel, parseModel, sameModel, type ModelSpec } from "../lib/model-spec";
import { readState, resolveScope, writeState } from "../lib/subagent-mode";

type WorkerName = "general" | "explore" | "astra";

type Worker = {
  // "mirror" workers follow the active mode. "fixed" workers only switch within
  // their own model family and always follow the invoking assistant's speed.
  kind: "mirror" | "fixed";
  // Specs used in auto mode, and for fixed workers in every mode.
  auto: { normal: ModelSpec; fast: ModelSpec };
  description?: string;
  reasoningEffort?: string;
};

const lunaMax = { providerID: "openai", modelID: "gpt-6-luna", variant: "max" } satisfies ModelSpec;
const solFast = (variant: "high" | "low"): ModelSpec => ({
  providerID: "openai",
  modelID: "gpt-6-sol-fast",
  variant,
});
const astra = { providerID: "openai", modelID: "gpt-6-astra" } satisfies ModelSpec;
const astraFast = { providerID: "openai", modelID: "gpt-6-astra-fast" } satisfies ModelSpec;

const workers = {
  general: { kind: "mirror", auto: { normal: lunaMax, fast: solFast("high") } },
  explore: { kind: "mirror", auto: { normal: lunaMax, fast: solFast("low") } },
  astra: {
    kind: "fixed",
    auto: { normal: astra, fast: astraFast },
    reasoningEffort: "low",
    description:
      "Difficult or high-stakes subtasks: architecture and design trade-offs, hard debugging, deep reviews, and nuanced writing. More expensive than the default workers.",
  },
} satisfies Record<WorkerName, Worker>;

const isWorker = (name: unknown): name is WorkerName =>
  typeof name === "string" && name in workers;

const childSuffix = "These workers are unavailable from child sessions.";

// Switching the worker or model on an existing child session drops its prompt
// cache and reprocesses all tokens, so resumes must keep the original model.
const pinPolicy =
  "A resumed child session keeps the worker and model of its first turn. To continue with a different worker or model, start a new task without task_id.";

const taskPolicy = `Delegation policy for task calls:
general and explore are the default workers. Choose astra yourself only in OpenAI main sessions; from any other provider, call it only when the user explicitly requests it.
${childSuffix}`;

const systemPolicy = (providerID: string) =>
  providerID === "openai"
    ? `Delegation policy for task calls (you are in an OpenAI main session):
general and explore are the default workers. You may choose astra yourself for difficult or high-stakes subtasks.
${pinPolicy}
${childSuffix}`
    : `Delegation policy for task calls (you are in a ${providerID} main session, not OpenAI):
your only workers are general and explore. NEVER delegate to astra unless the latest user message explicitly names "astra".
${pinPolicy}
${childSuffix}`;

// The task prompt is the only channel that reaches the child session, so the
// routing decision rides along as a marker and is stripped again in chat.message.
type Marker = { fast: boolean } | { model: ModelSpec };

export default (async ({ client }) => {
  const prefix = `<opencode-orchestrator-${crypto.randomUUID()}:`;
  const encodeMarker = (marker: Marker) => `${prefix}${JSON.stringify(marker)}>\n`;
  const decodeMarker = (agent: string | undefined, text: string): { marker: Marker; length: number } => {
    const end = text.indexOf(">\n", prefix.length);
    let raw: unknown;
    if (end !== -1) {
      try {
        raw = JSON.parse(text.slice(prefix.length, end));
      } catch {
        raw = undefined;
      }
    }
    if (typeof raw === "object" && raw !== null) {
      const { fast, model } = raw as { fast?: unknown; model?: unknown };
      if (typeof fast === "boolean" && model === undefined)
        return { marker: { fast }, length: end + 2 };
      const spec = asModelSpec(model);
      if (fast === undefined && spec !== undefined)
        return { marker: { model: spec }, length: end + 2 };
    }
    throw new Error(`Invalid orchestrator model marker for agent ${agent}`);
  };

  // Each child session is pinned to the model of its first turn: resuming it
  // with another worker (general <-> astra) or speed drops prompt cache and
  // reprocesses all tokens, so such resumes are blocked and must start a new
  // task without task_id instead. New children have no ID until the task tool
  // returns, so specs wait in pendingModels keyed by parent session and call
  // (call IDs alone are not unique across sessions) and move over in
  // tool.execute.after.
  const childModels = new Map<string, ModelSpec>();
  const pendingModels = new Map<string, ModelSpec>();

  // First non-empty string under any of these keys.
  const strField = (obj: Record<string, unknown>, keys: readonly string[]): string | undefined => {
    for (const key of keys) {
      const value = obj[key];
      if (typeof value === "string" && value.length > 0) return value;
    }
    return undefined;
  };

  // task_id is the v1 resume key; the sessionID spellings cover the v2
  // subagent tool and metadata variants so pins apply across versions.
  const resumeID = (args: Record<string, unknown>): string | undefined =>
    strField(args, ["task_id", "taskId", "sessionID", "sessionId", "session_id"]);

  const explicitModel = (args: Record<string, unknown>): ModelSpec | undefined => {
    const value = strField(args, ["model"]);
    return value === undefined ? undefined : parseModel(value.trim());
  };

  // Best-effort lookup of a child created before this process started (or
  // before the after-hook recorded it). Never throws; unknown means first-seen.
  const fetchChildModel = async (childID: string): Promise<ModelSpec | undefined> => {
    let messages: Array<{ info: unknown }>;
    try {
      messages =
        (
          await client.session.messages({
            path: { id: childID },
            query: { limit: 50 },
            throwOnError: true,
          })
        ).data ?? [];
    } catch {
      return undefined;
    }
    for (let index = messages.length - 1; index >= 0; index--) {
      const info = messages[index].info as Record<string, unknown> | undefined;
      if (typeof info !== "object" || info === null) continue;
      if (
        info.role === "assistant" &&
        typeof info.providerID === "string" &&
        typeof info.modelID === "string"
      ) {
        return { providerID: info.providerID, modelID: info.modelID };
      }
      const spec = asModelSpec((info as { model?: unknown }).model);
      if (spec !== undefined) return { providerID: spec.providerID, modelID: spec.modelID };
    }
    return undefined;
  };

  const checkModelPin = async (
    args: Record<string, unknown>,
    desired: ModelSpec,
    workerName: WorkerName,
    sessionID: string,
    callID: string,
  ): Promise<void> => {
    const taskID = resumeID(args);
    if (taskID === undefined) {
      // Bound the table: cancelled calls never reach tool.execute.after.
      if (pendingModels.size > 1000) pendingModels.delete(pendingModels.keys().next().value!);
      pendingModels.set(`${sessionID}:${callID}`, desired);
      return;
    }
    let pinned = childModels.get(taskID);
    if (pinned === undefined) {
      pinned = (await fetchChildModel(taskID)) ?? desired;
      childModels.set(taskID, pinned);
    }
    const explicit = explicitModel(args);
    if (explicit !== undefined && !sameModel(explicit, pinned)) {
      throw new Error(
        `Cannot resume subagent session ${taskID} with explicit model ${formatModel(explicit)}: it is pinned to ${formatModel(pinned)}. Continue with the same model, or start a new task without task_id for a different model.`,
      );
    }
    if (!sameModel(desired, pinned)) {
      throw new Error(
        `Cannot resume subagent session ${taskID} with ${workerName} (${formatModel(desired)}): it is pinned to ${formatModel(pinned)}. Model switches drop prompt cache and reprocess all tokens. Continue with the same worker and speed, or start a new task without task_id for a different worker or model.`,
      );
    }
  };

  // The assistant that invoked a task owns its model. Queued user messages can be
  // newer than it, and command subtasks record the target model on a synthetic
  // assistant, so both need explicit resolution.
  const resolveInvoker = async (sessionID: string, callID: string, worker: string) => {
    const { data: session } = await client.session.get({
      path: { id: sessionID },
      throwOnError: true,
    });
    if (session.parentID) {
      throw new Error(`${worker} is available only from main sessions`);
    }

    const page = (limit: number) =>
      client.session.messages({
        path: { id: sessionID },
        query: { limit },
        throwOnError: true,
      });
    let limit = 2;
    let messages = (await page(limit)).data;
    let current = messages.findLast(({ info }) => info.role === "assistant");
    // The runner persists its assistant before tool execution. Queued users can be
    // newer, so widen the page until the assistant appears or the page is not full.
    while (!current && messages.length >= limit) {
      limit *= 2;
      messages = (await page(limit)).data;
      current = messages.findLast(({ info }) => info.role === "assistant");
    }
    const assistant = current?.info;
    if (
      !current ||
      assistant?.role !== "assistant" ||
      assistant.time.completed !== undefined ||
      assistant.summary
    ) {
      throw new Error(
        `Unable to resolve executing assistant for task ${callID} in session ${sessionID}`,
      );
    }

    // Command subtasks pass the persisted part ID. Their invoking model belongs to
    // the parent user, not the target model on the synthetic assistant.
    if (
      current.parts.some(
        (part) => part.type === "tool" && part.tool === "task" && part.id === callID,
      )
    ) {
      const parent =
        messages.find(({ info }) => info.id === assistant.parentID) ??
        (
          await client.session.message({
            path: { id: sessionID, messageID: assistant.parentID },
            throwOnError: true,
          })
        ).data;
      if (parent.info.role !== "user") {
        throw new Error(
          `Unable to resolve invoking user for command task ${callID} in session ${sessionID}`,
        );
      }
      return parent.info.model;
    }
    return { providerID: assistant.providerID, modelID: assistant.modelID };
  };

  return {
    config: async (config) => {
      config.agent ??= {};
      const existing = config.agent.astra as { options?: Record<string, unknown> } | undefined;
      config.agent.astra = {
        description: workers.astra.description,
        mode: "subagent",
        model: `${workers.astra.auto.normal.providerID}/${workers.astra.auto.normal.modelID}`,
        ...existing,
        options: { reasoningEffort: workers.astra.reasoningEffort, ...existing?.options },
      };
    },
    // Overrides live in a state file owned by the /subagents dialog; keep it from
    // accumulating scopes for sessions that no longer exist.
    event: async ({ event }) => {
      if (event.type !== "session.deleted") return;
      const id = event.properties.info.id;
      childModels.delete(id);
      for (const key of pendingModels.keys()) if (key.startsWith(`${id}:`)) pendingModels.delete(key);
      const state = readState();
      if (state.sessions === undefined || !(id in state.sessions)) return;
      delete state.sessions[id];
      writeState(state);
    },
    "tool.definition": async ({ toolID }, output) => {
      if (toolID === "task") output.description += `\n\n${taskPolicy}`;
    },
    "experimental.chat.system.transform": async ({ model }, output) => {
      const providerID = (model as { providerID?: unknown } | undefined)?.providerID;
      const policy = systemPolicy(typeof providerID === "string" && providerID ? providerID : "unknown");
      // Qwen templates require a single initial system message.
      output.system.splice(0, output.system.length, [...output.system, policy].join("\n\n"));
    },
    "tool.execute.before": async ({ tool, sessionID, callID }, output) => {
      if (tool !== "task" || !isWorker(output.args.subagent_type)) return;
      const workerName = output.args.subagent_type;
      const worker = workers[workerName];
      const args = output.args as Record<string, unknown>;

      const { providerID, modelID } = await resolveInvoker(sessionID, callID, workerName);
      const { mode, models } = resolveScope(readState(), sessionID);
      // Fixed workers keep their own family, and other providers keep the inherited
      // model in auto mode, so only mirror workers in a forced mode carry a model.
      const forced = worker.kind === "mirror" && mode !== "auto" ? models[mode] : undefined;
      const marker: Marker | undefined =
        forced !== undefined
          ? { model: forced }
          : providerID === "openai"
            ? { fast: modelID.endsWith("-fast") }
            : undefined;
      const desired: ModelSpec =
        marker === undefined
          ? { providerID, modelID }
          : "model" in marker
            ? marker.model
            : worker.auto[marker.fast ? "fast" : "normal"];
      // Every delegation pins its model. Marked invocations also carry the routing
      // snapshot in the prompt (stripped in chat.message) because task has no
      // model override or child-prompt correlation ID of its own.
      await checkModelPin(args, desired, workerName, sessionID, callID);
      if (marker !== undefined) output.args.prompt = encodeMarker(marker) + output.args.prompt;
    },
    "tool.execute.after": async ({ tool, sessionID, callID }, output) => {
      if (tool !== "task") return;
      const key = `${sessionID}:${callID}`;
      const pending = pendingModels.get(key);
      pendingModels.delete(key);
      if (pending === undefined) return;
      const metadata = (output as unknown as { metadata?: unknown } | undefined)?.metadata;
      const childID =
        typeof metadata === "object" && metadata !== null
          ? strField(metadata as Record<string, unknown>, ["sessionId", "sessionID", "session_id"])
          : undefined;
      if (childID !== undefined && !childModels.has(childID)) childModels.set(childID, pending);
    },
    "chat.message": async ({ agent }, { message, parts }) => {
      const part = parts.find(
        (part): part is Extract<typeof part, { type: "text" }> =>
          part.type === "text" && part.text.startsWith(prefix),
      );
      if (!part) return;
      const { marker, length } = decodeMarker(agent, part.text);
      // Remove transport metadata before the child message is persisted or sent to an LLM.
      part.text = part.text.slice(length);
      if (!isWorker(agent)) return;
      const worker = workers[agent];
      const spec = "model" in marker ? marker.model : worker.auto[marker.fast ? "fast" : "normal"];
      const current = message.model as ModelSpec;
      // Fixed workers only rewrite their own family, leaving explicit models from
      // recovery hooks authoritative. Mirror workers are always rewritten.
      if (
        worker.kind === "fixed" &&
        !sameModel(current, worker.auto.normal) &&
        !sameModel(current, worker.auto.fast)
      ) {
        return;
      }
      current.providerID = spec.providerID;
      current.modelID = spec.modelID;
      if (worker.kind === "mirror") current.variant = spec.variant;
    },
  };
}) satisfies Plugin;
