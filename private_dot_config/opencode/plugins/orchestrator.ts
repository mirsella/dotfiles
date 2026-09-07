import type { Plugin } from "@opencode-ai/plugin";

const tierNames = ["luna", "sol", "astra"] as const;
type Tier = (typeof tierNames)[number];
type TierPolicy = { agents: readonly string[]; guidance: string };

const tiers: Record<Tier, TierPolicy> = {
  luna: {
    agents: ["luna"],
    guidance: "use luna only",
  },
  sol: {
    agents: ["luna", "sol"],
    guidance:
      "use luna for routine work and sol for difficult, ambiguous, sensitive, or high-risk work",
  },
  astra: {
    agents: ["luna", "sol", "astra"],
    guidance:
      "use luna for routine work, sol for difficult work, and astra for the hardest or highest-risk work",
  },
};

const tierAgents = new Set(Object.values(tiers).flatMap(({ agents }) => agents));

export default (async () => {
  const sessions = new Map<string, Tier>();

  return {
    config: async (config) => {
      config.agent ??= {};
      config.agent.luna = {
        description: "Luna with maximum reasoning.",
        mode: "subagent",
        model: "openai/gpt-5.6-luna",
        options: { reasoningEffort: "max" },
      };
      config.agent.sol = {
        description: "Sol with high reasoning.",
        mode: "subagent",
        model: "openai/gpt-5.6-sol",
        options: { reasoningEffort: "high" },
      };
      config.agent.astra = {
        description: "Astra with medium reasoning.",
        mode: "subagent",
        model: "openai/gpt-6-astra",
        options: { reasoningEffort: "medium" },
      };
    },
    "experimental.chat.system.transform": async ({ sessionID, model }, output) => {
      if (!sessionID) return;
      const modelParts = model.id.split("-");
      const tier = tierNames.find((name) => modelParts.includes(name));
      if (!tier) {
        sessions.delete(sessionID);
        return;
      }
      sessions.set(sessionID, tier);
      output.system.push(
        `Delegation: ${tiers[tier].guidance}. Never use general. Honor explicit choices; ask if unavailable.`,
      );
    },
    "tool.execute.before": async ({ tool, sessionID }, output) => {
      if (tool !== "task") return;
      const agent = output.args.subagent_type;
      if (typeof agent !== "string") return;
      const tier = sessions.get(sessionID);
      if (agent === "general" && tier) throw new Error(`general is unavailable from the ${tier} tier`);
      if (!tierAgents.has(agent)) return;
      if (!tier) throw new Error(`${agent} is unavailable for the active model`);
      if (!tiers[tier].agents.includes(agent)) throw new Error(`${agent} is unavailable from the ${tier} tier`);
    },
    event: async ({ event }) => {
      if (event.type === "session.deleted") sessions.delete(event.properties.info.id);
    },
  };
}) satisfies Plugin;
