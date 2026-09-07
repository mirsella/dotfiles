import { expect, test } from "bun:test";
import type { Config } from "@opencode-ai/plugin";
import orchestrator from "../plugins/orchestrator";

test("registers the three model-only tier agents and preserves other configuration", async () => {
  const general = { mode: "subagent" as const };
  const config: Config = { model: "openai/gpt-6-astra", agent: { general } };
  const hooks = await orchestrator();
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.agent).toEqual({
    general,
    luna: {
      description: "Luna with maximum reasoning.",
      mode: "subagent",
      model: "openai/gpt-5.6-luna",
      options: { reasoningEffort: "max" },
    },
    sol: {
      description: "Sol with high reasoning.",
      mode: "subagent",
      model: "openai/gpt-5.6-sol",
      options: { reasoningEffort: "high" },
    },
    astra: {
      description: "Astra with medium reasoning.",
      mode: "subagent",
      model: "openai/gpt-6-astra",
      options: { reasoningEffort: "medium" },
    },
  });
  expect(config.agent!.general).toBe(general);
});

test("injects concise model-tier instructions and enforces access", async () => {
  const hooks = await orchestrator();
  const transform = hooks["experimental.chat.system.transform"];
  const before = hooks["tool.execute.before"];
  const model = (id: string) => ({ id }) as Parameters<typeof transform>[0]["model"];
  const inject = async (sessionID: string, id: string) => {
    const output = { system: ["base"] };
    await transform({ sessionID, model: model(id) }, output);
    return output.system;
  };
  const task = (sessionID: string, subagent_type: string) =>
    before({ tool: "task", sessionID, callID: "call" }, { args: { subagent_type } });

  const cases = [
    {
      tier: "luna",
      model: "gpt-5.6-luna",
      allowed: ["luna"],
      denied: ["sol", "astra"],
      guidance: "use luna only",
    },
    {
      tier: "sol",
      model: "gpt-5.6-sol-fast",
      allowed: ["luna", "sol"],
      denied: ["astra"],
      guidance:
        "use luna for routine work and sol for difficult, ambiguous, sensitive, or high-risk work",
    },
    {
      tier: "astra",
      model: "gpt-6-astra",
      allowed: ["luna", "sol", "astra"],
      denied: [],
      guidance:
        "use luna for routine work, sol for difficult work, and astra for the hardest or highest-risk work",
    },
  ];

  for (const { tier, model: modelID, allowed, denied, guidance } of cases) {
    expect(await inject(tier, modelID)).toEqual([
      "base",
      `Delegation: ${guidance}. Never use general. Honor explicit choices; ask if unavailable.`,
    ]);
    for (const agent of allowed) await expect(task(tier, agent)).resolves.toBeUndefined();
    for (const agent of denied) {
      await expect(task(tier, agent)).rejects.toThrow(`unavailable from the ${tier} tier`);
    }
    await expect(task(tier, "general")).rejects.toThrow(`unavailable from the ${tier} tier`);
    await expect(task(tier, "explore")).resolves.toBeUndefined();
  }

  expect(await inject("other", "consolidated-model")).toEqual(["base"]);
  for (const agent of ["luna", "sol", "astra"]) {
    await expect(task("other", agent)).rejects.toThrow("unavailable for the active model");
  }
  await expect(task("other", "general")).resolves.toBeUndefined();
});
