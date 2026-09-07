import type { Plugin } from "@opencode-ai/plugin";

const workers = [
  {
    name: "luna",
    description: "Handles routine exploration, research, implementation, and checks.",
    model: "openai/gpt-5.6-luna",
    reasoningEffort: "max",
  },
  {
    name: "sol",
    description: "Handles difficult investigation, implementation, debugging, and review.",
    model: "openai/gpt-5.6-sol",
    reasoningEffort: "high",
  },
  {
    name: "astra",
    description: "Handles the hardest problems and offers independent ideas, feedback, and review.",
    model: "openai/gpt-6-astra",
    reasoningEffort: "medium",
  },
] as const;

export default (async ({ client }) => {
  return {
    config: async (config) => {
      config.agent ??= {};
      for (const { name, description, model, reasoningEffort } of workers) {
        const existing = config.agent[name];
        config.agent[name] = {
          description,
          mode: "subagent",
          model,
          ...existing,
          options: { reasoningEffort, ...(existing?.options as Record<string, unknown> | undefined) },
        };
      }
    },
    "tool.definition": async ({ toolID }, output) => {
      if (toolID !== "task") return;
      output.description += `\n\nDelegation policy:
luna, sol, and astra are available only from top-level OpenAI sessions, subject to permissions and disabled agents. In those sessions, do not use general. Respect explicit agent and cost constraints. Report an unavailable requested agent; otherwise a legitimately available alternative is allowed, without bypassing restrictions.
Default to luna for routine work and sol for difficult work; they should handle most delegated work. Use astra for the hardest or highest-risk problems, or a useful fresh perspective, ideas, feedback, or review. Select by the assignment, not the parent's tier; escalation does not require a separate user request. These are defaults, not quotas or mandatory reviews.
Balance quality, elapsed time, and total effort, including briefing and review. Same-tier delegation adds capacity and perspective but requires context transfer. Parallelize independent scopes with non-overlapping writes; reuse existing task IDs when their context helps. Avoid accidental duplication; deliberate comparison and verification are valid.
Load the orchestrator skill for multi-agent coordination. When the user explicitly requests orchestration, delegate substantive exploration, research, implementation, and review. The parent owns shared decisions and integration: understand the implementation, assess tradeoffs, and inspect key code rather than just collecting reports. Delegated design and small local edits or unblockers are appropriate.`;
    },
    "tool.execute.before": async ({ tool, sessionID, callID }, output) => {
      if (tool !== "task") return;
      const agent = output.args.subagent_type;
      if (agent !== "general" && !workers.some(({ name }) => name === agent)) return;

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

      let providerID = assistant.providerID;
      // Command subtasks pass the persisted part ID and record the target model on
      // their synthetic assistant. Their invoking model belongs to the parent user.
      if (current.parts.some((part) => part.type === "tool" && part.tool === "task" && part.id === callID)) {
        const parent = messages.find(({ info }) => info.id === assistant.parentID)
          ?? (await client.session.message({ path: { id: sessionID, messageID: assistant.parentID }, throwOnError: true })).data;
        if (parent.info.role !== "user") {
          throw new Error(`Unable to resolve invoking user for command task ${callID} in session ${sessionID}`);
        }
        providerID = parent.info.model.providerID;
      }

      const openaiMain = !session.data.parentID && providerID === "openai";
      if (agent === "general" && openaiMain) throw new Error("general is unavailable from OpenAI main sessions");
      if (agent !== "general" && !openaiMain) {
        throw new Error(`${agent} is available only from OpenAI main sessions`);
      }
    },
  };
}) satisfies Plugin;
