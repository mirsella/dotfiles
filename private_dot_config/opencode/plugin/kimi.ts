// @ts-nocheck

/**
 * Kimi Key Chooser Plugin for OpenCode (Kimi For Coding only)
 *
 * Creates separate provider entries for each Kimi API key found in
 * environment variables starting with KIMI_API_KEY.
 *
 * Specification (Kimi For Coding):
 * - SDK: @ai-sdk/anthropic
 * - API: https://api.kimi.com/coding/v1
 * - Model: k2p5 (Kimi K2.5)
 */

export default async (context) => {
  const prefix = "KIMI_API_KEY";

  return {
    config: async (cfg) => {
      const envKeys = Object.keys(process.env).filter((k) =>
        k.startsWith(prefix),
      );
      if (envKeys.length === 0) return;

      if (!cfg.provider) cfg.provider = {};

      for (const k of envKeys) {
        let suffix = k.slice(prefix.length);
        if (suffix.startsWith("_")) suffix = suffix.slice(1);

        const label = suffix || "Default";
        const key = process.env[k];
        if (!key || key.length < 5) continue;

        // Unique provider ID for each key to ensure isolation
        const providerID = `kimi-coding-${(suffix || "default").toLowerCase()}`;

        cfg.provider[providerID] = {
          name: `Kimi For Coding (${label})`,
          api: "https://api.kimi.com/coding/v1",
          npm: "@ai-sdk/anthropic",
          options: {
            apiKey: key,
            headers: {
              // Standard Kimi for Coding beta headers for reasoning and streaming tools
              "anthropic-beta":
                "claude-code-20250219,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14",
            },
          },
          models: {
            "kimi-k2.5": {
              name: "Kimi K2.5",
              id: "k2p5", // Actual API identifier for the model
              reasoning: true,
              limit: {
                context: 128000,
                output: 8192,
              },
            },
          },
        };
      }
    },
  };
};
