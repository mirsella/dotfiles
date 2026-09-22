import type { Plugin } from "@opencode-ai/plugin";
import { asModelSpec, type ModelSpec } from "../lib/model-spec";
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

const lunaMax = { providerID: "openai", modelID: "gpt-5.6-luna", variant: "max" } satisfies ModelSpec;
const solFast = (variant: "high" | "low"): ModelSpec => ({
  providerID: "openai",
  modelID: "gpt-5.6-sol-fast",
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

const workerFor = (name?: string): Worker | undefined =>
  name === undefined ? undefined : (workers as Record<string, Worker | undefined>)[name];

const taskPolicy = `Delegation policy for task calls:
general and explore are the default workers. Choose astra yourself only in OpenAI main sessions; from any other provider, call it only when the user explicitly requests it.
These workers are unavailable from child sessions.`;

// The task prompt is the only channel that reaches the child session, so the
// routing decision rides along as a marker and is stripped again in chat.message.
type Marker = { fast: boolean } | { model: ModelSpec };

export default (async ({ client }) => {
  const prefix = `<opencode-orchestrator-${crypto.randomUUID()}:`;
  const encodeMarker = (marker: Marker) => `${prefix}${JSON.stringify(marker)}>\n`;
  const decodeMarker = (agent: string | undefined, text: string): { marker: Marker; length: number } => {
    const invalid = () => new Error(`Invalid orchestrator model marker for agent ${agent}`);
    const end = text.indexOf(">\n", prefix.length);
    if (end === -1) throw invalid();
    let raw: unknown;
    try {
      raw = JSON.parse(text.slice(prefix.length, end));
    } catch {
      throw invalid();
    }
    if (typeof raw !== "object" || raw === null) throw invalid();
    const { fast, model } = raw as Record<string, unknown>;
    if (fast === undefined) {
      const spec = asModelSpec(model);
      if (spec === undefined) throw invalid();
      return { marker: { model: spec }, length: end + 2 };
    }
    if (model !== undefined || typeof fast !== "boolean") throw invalid();
    return { marker: { fast }, length: end + 2 };
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
      const existing = config.agent.astra;
      config.agent.astra = {
        description: workers.astra.description,
        mode: "subagent",
        model: `${workers.astra.auto.normal.providerID}/${workers.astra.auto.normal.modelID}`,
        ...existing,
        options: {
          reasoningEffort: workers.astra.reasoningEffort,
          ...(existing?.options as Record<string, unknown> | undefined),
        },
      };
    },
    // Overrides live in a state file owned by the /subagents dialog; keep it from
    // accumulating scopes for sessions that no longer exist.
    event: async ({ event }) => {
      if (event.type !== "session.deleted") return;
      const state = readState();
      if (state.sessions === undefined || !(event.properties.info.id in state.sessions)) return;
      delete state.sessions[event.properties.info.id];
      writeState(state);
    },
    "tool.definition": async ({ toolID }, output) => {
      if (toolID === "task") output.description += `\n\n${taskPolicy}`;
    },
    "tool.execute.before": async ({ tool, sessionID, callID }, output) => {
      if (tool !== "task") return;
      const requested = output.args.subagent_type as string;
      const worker = workerFor(requested);
      if (!worker) return;

      const { providerID, modelID } = await resolveInvoker(sessionID, callID, requested);
      const { mode, models } = resolveScope(readState(), sessionID);
      // Fixed workers keep their own family, and other providers keep the inherited
      // model in auto mode, so only mirror workers in a forced mode carry a model.
      const forced = worker.kind === "mirror" && mode !== "auto" ? models[mode] : undefined;
      if (forced === undefined && providerID !== "openai") return;

      // Task has no model override or child-prompt correlation ID. Carry the
      // snapshot with this invocation so cancelled/queued resumes cannot mix it up.
      output.args.prompt =
        encodeMarker(
          forced === undefined
            ? { fast: providerID === "openai" && modelID.endsWith("-fast") }
            : { model: forced },
        ) + output.args.prompt;
    },
    "chat.message": async ({ agent }, { message, parts }) => {
      const part = parts.find(
        (part) => part.type === "text" && part.text.startsWith(prefix),
      );
      if (part?.type !== "text") return;
      const { marker, length } = decodeMarker(agent, part.text);
      // Remove transport metadata before the child message is persisted or sent to an LLM.
      part.text = part.text.slice(length);
      const worker = workerFor(agent);
      if (!worker) return;
      const spec = "model" in marker ? marker.model : worker.auto[marker.fast ? "fast" : "normal"];
      const current = message.model as ModelSpec;
      // Fixed workers only rewrite their own family, leaving explicit models from
      // recovery hooks authoritative. Mirror workers are always rewritten.
      if (
        worker.kind === "fixed" &&
        (current.providerID !== spec.providerID ||
          (current.modelID !== worker.auto.normal.modelID &&
            current.modelID !== worker.auto.fast.modelID))
      ) {
        return;
      }
      current.providerID = spec.providerID;
      current.modelID = spec.modelID;
      if (worker.kind === "mirror") current.variant = spec.variant;
    },
  };
}) satisfies Plugin;
