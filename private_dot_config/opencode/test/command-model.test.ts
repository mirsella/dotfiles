import { expect, test } from "bun:test";
import type { Config } from "@opencode-ai/plugin";
import commandModel from "../plugins/command-model";

test("command overrides preserve fast mode and apply once per session", async () => {
  const hooks = await commandModel();
  const astra = { providerID: "openai", modelID: "gpt-6-astra", variant: "high" };
  const luna = { providerID: "openai", modelID: "gpt-5.6-luna", variant: "max" };
  const config = {
    model: "openai/gpt-6-astra",
    command: {
      commit: { template: "Commit this session's changes", model: "openai/gpt-5.6-luna#max" },
      plain: { template: "No thinking override", model: "openai/gpt-5.6-luna" },
      fast: { template: "Explicit fast", model: "openai/gpt-5.6-luna-fast#max" },
      other: { template: "Other provider", model: "other/model#high" },
      review: { template: "No model override" },
      commitdiff: { template: "Commit diff", subtask: true, model: "openai/gpt-5.6-luna" },
    },
  } satisfies Config;
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.command.commit).toEqual({ template: "Commit this session's changes" });
  expect(config.command.plain.model).toBeUndefined();
  expect(config.command.commitdiff.model).toBe("openai/gpt-5.6-luna");
  const command = (sessionID: string, name = "commit") =>
    hooks["command.execute.before"](
      { sessionID, command: name, arguments: "" },
      { parts: [] },
    );
  const chat = async (sessionID: string, source = astra, plugin = hooks) => {
    const output: Parameters<typeof hooks["chat.message"]>[1] = {
      message: {
        id: "msg_test",
        sessionID,
        role: "user",
        time: { created: 0 },
        agent: "build",
        model: source,
      },
      parts: [],
    };
    await plugin["chat.message"]({ sessionID, model: source }, output);
    expect(output.message.sessionID).toBe(sessionID);
    expect(output.message.agent).toBe("build");
    expect(output.parts).toEqual([]);
    return output.message.model;
  };

  await Promise.all([command("a"), command("b")]);
  expect(await chat("unmarked")).toBe(astra);
  expect(await chat("b")).toEqual(luna);
  expect(await chat("b")).toBe(astra);
  expect(await chat("a")).toEqual(luna);
  expect(await chat("a")).toBe(astra);
  expect(astra.modelID).toBe("gpt-6-astra");
  expect(astra.variant).toBe("high");

  await command("a", "plain");
  expect(await chat("a")).toEqual({ providerID: "openai", modelID: "gpt-5.6-luna" });
  expect(await chat("a")).toBe(astra);

  await command("a", "review");
  expect(await chat("a")).toBe(astra);
  await command("a", "commitdiff");
  expect(await chat("a")).toBe(astra);
  await command("a");
  await command("a", "review");
  expect(await chat("a")).toBe(astra);

  await command("a");
  expect(await chat("a", astra, await commandModel())).toBe(astra);
  expect(await chat("a")).toEqual(luna);

  for (const [name, providerID, modelID, expectedProvider, expectedModel, variant] of [
    ["commit", "openai", "gpt-6-astra-fast", "openai", "gpt-5.6-luna-fast", "max"],
    ["commit", "openai", "gpt-5.6-sol-fast", "openai", "gpt-5.6-luna-fast", "max"],
    ["commit", "openai", "gpt-6-astra", "openai", "gpt-5.6-luna", "max"],
    ["commit", "other", "model-fast", "openai", "gpt-5.6-luna", "max"],
    ["fast", "openai", "gpt-6-astra-fast", "openai", "gpt-5.6-luna-fast", "max"],
    ["fast", "openai", "gpt-6-astra", "openai", "gpt-5.6-luna-fast", "max"],
    ["other", "openai", "gpt-6-astra-fast", "other", "model", "high"],
    ["plain", "openai", "gpt-6-astra-fast", "openai", "gpt-5.6-luna-fast", undefined],
  ] as const) {
    const source = Object.freeze({ providerID, modelID, variant: "medium" });
    await command("a", name);
    expect(await chat("a", source)).toEqual({ providerID: expectedProvider, modelID: expectedModel, variant });
    expect(source).toEqual({ providerID, modelID, variant: "medium" });
    expect(await chat("a", source)).toBe(source);
  }
});

test("rejects malformed command models", async () => {
  const hooks = await commandModel();
  for (const model of ["luna", "openai/", "/luna", "openai/luna#", "openai/luna#max#high"]) {
    await expect(hooks.config({ command: { commit: { template: "Commit", model } } }))
      .rejects.toThrow("expected provider/model[#variant]");
  }
});
