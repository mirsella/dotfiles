import type { Plugin } from "@opencode-ai/plugin";

type ModelSpec = {
  providerID: string;
  modelID: string;
  variant?: string;
};

type WorkerName = "general" | "explore" | "astra";

type Worker = {
  // "mirror" workers have no configured model, so their child model is inherited
  // from the invoking session and the plugin always replaces it. "fixed" workers
  // declare a model in agent config and only switch within that family.
  kind: "mirror" | "fixed";
  normal: ModelSpec;
  fast: ModelSpec;
  description?: string;
  reasoningEffort?: string;
};

const deepseek = { providerID: "opencode-go", modelID: "deepseek-v4.1-flash" } satisfies ModelSpec;
const solFast = (variant: "high" | "low"): ModelSpec => ({
  providerID: "openai",
  modelID: "gpt-5.6-sol-fast",
  variant,
});

const workers = {
  general: { kind: "mirror", normal: deepseek, fast: solFast("high") },
  explore: { kind: "mirror", normal: deepseek, fast: solFast("low") },
  astra: {
    kind: "fixed",
    normal: { providerID: "openai", modelID: "gpt-6-astra" },
    fast: { providerID: "openai", modelID: "gpt-6-astra-fast" },
    reasoningEffort: "low",
    description:
      "Difficult, ambiguous, or high-stakes subtasks: architecture and design trade-offs, technical guidance, hard implementation or debugging, deep reviews (correctness, concurrency, security, performance), and nuanced writing (issues, PRs, support replies, emails). faster but more expensive.",
  },
} satisfies Record<WorkerName, Worker>;

const workerFor = (name?: string): Worker | undefined =>
  name === undefined ? undefined : (workers as Record<string, Worker | undefined>)[name];

const taskPolicy = `Delegation policy for task calls:
general and explore are the default workers; use them for routine, well-scoped subtasks, searches, and mechanical work.
astra covers difficult or high-stakes subtasks. In an OpenAI main session you may choose astra yourself when its description fits. From any other provider, call astra only when the user explicitly requests it.
These workers are unavailable from child sessions. Other agents are unaffected.`;

export default (async ({ client }) => {
  const prefix = `<opencode-orchestrator-${crypto.randomUUID()}:`;
  const markers = { normal: `${prefix}normal>\n`, fast: `${prefix}fast>\n` };

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
        model: `${workers.astra.normal.providerID}/${workers.astra.normal.modelID}`,
        ...existing,
        options: {
          reasoningEffort: workers.astra.reasoningEffort,
          ...(existing?.options as Record<string, unknown> | undefined),
        },
      };
    },
    "tool.definition": async ({ toolID }, output) => {
      if (toolID === "task") output.description += `\n\n${taskPolicy}`;
    },
    "tool.execute.before": async ({ tool, sessionID, callID }, output) => {
      if (tool !== "task") return;
      const requested = output.args.subagent_type as string;
      if (!workerFor(requested)) return;

      const { providerID, modelID } = await resolveInvoker(sessionID, callID, requested);
      // Other providers keep the inherited model for mirror workers and the
      // configured model for fixed workers, so there is nothing to carry.
      if (providerID !== "openai") return;

      // Task has no model override or child-prompt correlation ID. Carry the
      // snapshot with this invocation so cancelled/queued resumes cannot mix it up.
      output.args.prompt =
        markers[modelID.endsWith("-fast") ? "fast" : "normal"] + output.args.prompt;
    },
    "chat.message": async ({ agent }, { message, parts }) => {
      const part = parts.find(
        (part) => part.type === "text" && part.text.startsWith(prefix),
      );
      if (part?.type !== "text") return;
      const fast = part.text.startsWith(markers.fast);
      if (!fast && !part.text.startsWith(markers.normal)) {
        throw new Error(`Invalid orchestrator model marker for agent ${agent}`);
      }
      // Remove transport metadata before the child message is persisted or sent to an LLM.
      part.text = part.text.slice(markers[fast ? "fast" : "normal"].length);
      // Recovery plugins can correct the worker after tool.execute.before.
      const worker = workerFor(agent);
      if (!worker) return;
      const spec = fast ? worker.fast : worker.normal;
      const current = message.model as ModelSpec;
      // Fixed workers only rewrite their own family, leaving explicit models from
      // recovery hooks authoritative. Mirror workers are always inherited.
      if (
        worker.kind === "fixed" &&
        (current.providerID !== spec.providerID ||
          (current.modelID !== worker.normal.modelID &&
            current.modelID !== worker.fast.modelID))
      ) {
        return;
      }
      current.providerID = spec.providerID;
      current.modelID = spec.modelID;
      if (worker.kind === "mirror") current.variant = spec.variant;
    },
  };
}) satisfies Plugin;
