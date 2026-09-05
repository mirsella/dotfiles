import { expect, test } from "bun:test";
import type { Config } from "@opencode-ai/plugin";
import orchestrator from "../plugins/orchestrator";

test("registers only a model-only lunamax agent and preserves other configuration", async () => {
  const general = { mode: "subagent" as const };
  const config: Config = { model: "openai/gpt-6-astra", agent: { general } };
  const hooks = await orchestrator();
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.agent).toEqual({
    general,
    lunamax: {
      description: "Luna with max reasoning. Use for easy or medium tasks, bulk work, and scoped reviews.",
      mode: "subagent",
      model: "openai/gpt-5.6-luna",
      options: { reasoningEffort: "max" },
    },
  });
  expect(config.agent!.general).toBe(general);
  const empty: Config = {};
  await hooks.config(empty);
  expect(Object.keys(empty.agent!)).toEqual(["lunamax"]);
});
