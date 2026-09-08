import type { Plugin } from "@opencode-ai/plugin";

const workers = [
  {
    name: "luna",
    description: "Routine subtasks.",
    modelID: "gpt-5.6-luna",
    reasoningEffort: "max",
  },
  {
    name: "sol",
    description: "Difficult subtasks.",
    modelID: "gpt-5.6-sol",
    reasoningEffort: "high",
  },
  {
    name: "astra",
    description: "The hardest or highest-risk subtasks.",
    modelID: "gpt-6-astra",
    reasoningEffort: "medium",
  },
] as const;

const taskPolicy = `Delegation policy for task calls:
Delegate as usual. When providerID is "openai" and the session has no parentID, use luna for routine subtasks, sol for difficult subtasks, and astra for the hardest or highest-risk subtasks instead of general. Otherwise, use general; the named workers are unavailable. Other agents are unaffected.
Load the orchestrator skill only when the user explicitly requests orchestration for the current task.`;

export default (async ({ client }) => {
  const marker = `<opencode-orchestrator-${crypto.randomUUID()}:`;

  return {
    config: async (config) => {
      config.agent ??= {};
      for (const { name, description, modelID, reasoningEffort } of workers) {
        const existing = config.agent[name];
        config.agent[name] = {
          description,
          mode: "subagent",
          model: `openai/${modelID}`,
          ...existing,
          options: { reasoningEffort, ...(existing?.options as Record<string, unknown> | undefined) },
        };
      }
    },
    "tool.definition": async ({ toolID }, output) => {
      if (toolID === "task") output.description += `\n\n${taskPolicy}`;
    },
    "tool.execute.before": async ({ tool, sessionID, callID }, output) => {
      if (tool !== "task") return;
      const agent = output.args.subagent_type;
      const worker = workers.find(({ name }) => name === agent);
      if (agent !== "general" && !worker) return;

      let limit = 2;
      const [session, initial] = await Promise.all([
        client.session.get({ path: { id: sessionID }, throwOnError: true }),
        client.session.messages({ path: { id: sessionID }, query: { limit }, throwOnError: true }),
      ]);
      let messages = initial.data;
      // The runner persists its assistant before tool execution. Queued users can be newer;
      // streamed tool parts need not be persisted yet, so they cannot identify normal calls.
      while (messages.length === limit && messages.every(({ info }) => info.role === "user")) {
        limit *= 2;
        ({ data: messages } = await client.session.messages({ path: { id: sessionID }, query: { limit }, throwOnError: true }));
      }
      const current = messages.findLast(({ info }) => info.role === "assistant");
      const assistant = current?.info;
      if (!current || assistant?.role !== "assistant" || assistant.time.completed !== undefined || assistant.summary) {
        throw new Error(`Unable to resolve executing assistant for task ${callID} in session ${sessionID}`);
      }

      let { providerID, modelID } = assistant;
      // Command subtasks pass the persisted part ID and record the target model on
      // their synthetic assistant. Their invoking model belongs to the parent user.
      if (current.parts.some((part) => part.type === "tool" && part.tool === "task" && part.id === callID)) {
        const parent = messages.find(({ info }) => info.id === assistant.parentID)
          ?? (await client.session.message({ path: { id: sessionID, messageID: assistant.parentID }, throwOnError: true })).data;
        if (parent.info.role !== "user") {
          throw new Error(`Unable to resolve invoking user for command task ${callID} in session ${sessionID}`);
        }
        ({ providerID, modelID } = parent.info.model);
      }

      const openaiMain = !session.data.parentID && providerID === "openai";
      if (!worker) {
        if (openaiMain) throw new Error("general is unavailable from OpenAI main sessions");
        return;
      }
      if (!openaiMain) {
        throw new Error(`${agent} is available only from OpenAI main sessions`);
      }
      // Task has no model override or child-prompt correlation ID. Carry the
      // snapshot with this invocation so cancelled/queued resumes cannot mix it up.
      output.args.prompt = `${marker}${modelID.endsWith("-fast") ? "fast" : "normal"}>\n${output.args.prompt}`;
    },
    "chat.message": async ({ agent }, { message, parts }) => {
      const part = parts.find((part) => part.type === "text" && part.text.startsWith(marker));
      if (part?.type !== "text") return;
      const fastHeader = `${marker}fast>\n`;
      const normalHeader = `${marker}normal>\n`;
      const fast = part.text.startsWith(fastHeader);
      if (!fast && !part.text.startsWith(normalHeader)) {
        throw new Error(`Invalid orchestrator model marker for agent ${agent}`);
      }
      // Remove transport metadata before the child message is persisted or sent to an LLM.
      part.text = part.text.slice((fast ? fastHeader : normalHeader).length);
      // Recovery plugins can correct the worker after tool.execute.before.
      const worker = workers.find(({ name }) => name === agent);
      if (!worker) return;
      const base = worker.modelID;
      // Explicit models outside the worker's default family remain authoritative.
      if (message.model.providerID !== "openai") return;
      if (message.model.modelID === base || message.model.modelID === `${base}-fast`) {
        message.model.modelID = `${base}${fast ? "-fast" : ""}`;
      }
    },
  };
}) satisfies Plugin;
