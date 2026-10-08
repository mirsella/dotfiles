import { expect, test } from "bun:test";
import type { Config } from "@opencode-ai/plugin";
import commandModel from "../plugins/command-model";

type Hooks = Awaited<ReturnType<typeof commandModel>>;
type ModelRef = { providerID: string; modelID: string; variant?: string };

const messageFor = (sessionID: string, model: ModelRef): Parameters<Hooks["chat.message"]>[1] => ({
  message: {
    id: "msg_test",
    sessionID,
    role: "user",
    time: { created: 0 },
    agent: "build",
    model,
  },
  parts: [],
});

test("command overrides preserve fast mode and apply once per session", async () => {
  const hooks = await commandModel();
  const astra = { providerID: "openai", modelID: "gpt-6-astra", variant: "high" };
  const luna = { providerID: "openai", modelID: "gpt-6-luna", variant: "max" };
  const config = {
    model: "openai/gpt-6-astra",
    command: {
      commit: { template: "Commit this session's changes", model: "openai/gpt-6-luna#max" },
      land: { template: "Commit, land on main or queue for later, and clean up", model: "openai/gpt-6-luna#max" },
      plain: { template: "No thinking override", model: "openai/gpt-6-luna" },
      fast: { template: "Explicit fast", model: "openai/gpt-6-luna-fast#max" },
      review: { template: "No model override" },
      commitdiff: { template: "Commit diff", subtask: true, model: "openai/gpt-6-luna" },
      subagentcmd: { template: "Subagent cmd", subagent: true, model: "openai/gpt-6-luna" } as unknown as Config["command"][string],
    },
  } satisfies Config;
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.command.commit).toEqual({ template: "Commit this session's changes" });
  expect(config.command.land).toEqual({ template: "Commit, land on main or queue for later, and clean up" });
  expect(config.command.plain.model).toBeUndefined();
  expect(config.command.commitdiff.model).toBe("openai/gpt-6-luna");
  expect((config.command.subagentcmd as unknown as { model?: string }).model).toBe("openai/gpt-6-luna");
  const command = (sessionID: string, name = "commit") =>
    hooks["command.execute.before"](
      { sessionID, command: name, arguments: "" },
      { parts: [] },
    );
  const chat = async (sessionID: string, source: ModelRef = astra, plugin: Hooks = hooks) => {
    const output = messageFor(sessionID, source);
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
  expect(await chat("a")).toEqual({ providerID: "openai", modelID: "gpt-6-luna" });
  expect(await chat("a")).toBe(astra);

  await command("a", "land");
  expect(await chat("a")).toEqual(luna);
  expect(await chat("a")).toBe(astra);
  await command("a", "subagentcmd");
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

  // openai command overrides preserve the source model's fast mode.
  for (const [name, modelID, expectedModelID, variant] of [
    ["commit", "gpt-6-astra-fast", "gpt-6-luna-fast", "max"],
    ["commit", "gpt-6-sol-fast", "gpt-6-luna-fast", "max"],
    ["commit", "gpt-6-astra", "gpt-6-luna", "max"],
    ["fast", "gpt-6-astra-fast", "gpt-6-luna-fast", "max"],
    ["fast", "gpt-6-astra", "gpt-6-luna-fast", "max"],
    ["plain", "gpt-6-astra-fast", "gpt-6-luna-fast", undefined],
  ] as const) {
    const source = Object.freeze({ providerID: "openai", modelID, variant: "medium" });
    await command("a", name);
    expect(await chat("a", source)).toEqual({ providerID: "openai", modelID: expectedModelID, variant });
    expect(source).toEqual({ providerID: "openai", modelID, variant: "medium" });
    expect(await chat("a", source)).toBe(source);
  }
});

test("command overrides never switch the session to another provider", async () => {
  const hooks = await commandModel();
  const config = {
    command: {
      commit: { template: "Commit this session's changes", model: "openai/gpt-6-luna#max" },
      other: { template: "Other provider", model: "other/model#high" },
    },
  } satisfies Config;
  await hooks.config(config);

  const run = async (name: string, source: ModelRef) => {
    await hooks["command.execute.before"](
      { sessionID: "s", command: name, arguments: "" },
      { parts: [] },
    );
    const output = messageFor("s", source);
    await hooks["chat.message"]({ sessionID: "s", model: source }, output);
    return output.message.model;
  };

  const anthropic = Object.freeze({ providerID: "anthropic", modelID: "claude-opus-4-6", variant: "high" });
  expect(await run("commit", anthropic)).toBe(anthropic);

  const openai = Object.freeze({ providerID: "openai", modelID: "gpt-6-astra", variant: "high" });
  expect(await run("other", openai)).toBe(openai);
  expect(await run("commit", openai)).toEqual({ providerID: "openai", modelID: "gpt-6-luna", variant: "max" });
});

test("rejects malformed command models", async () => {
  const hooks = await commandModel();
  for (const model of ["luna", "openai/", "/luna", "openai/luna#", "openai/luna#max#high"]) {
    await expect(hooks.config({ command: { commit: { template: "Commit", model } } }))
      .rejects.toThrow("expected provider/model[#variant]");
  }
});
