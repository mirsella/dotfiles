import type { Plugin } from "@opencode-ai/plugin";

export default (async () => ({
  config: async (config) => {
    config.agent ??= {};
    config.agent.lunamax = {
      description: "Luna with max reasoning. Use for easy or medium tasks, bulk work, and scoped reviews.",
      mode: "subagent",
      model: "openai/gpt-5.6-luna",
      options: { reasoningEffort: "max" },
    };
  },
})) satisfies Plugin;
