import { expect, test } from "bun:test";
import type { Config } from "@opencode-ai/plugin";
import commandModel from "../plugins/command-model";

test("command model is consumed once and isolated between sessions", async () => {
  const hooks = await commandModel();
  const astra = { providerID: "openai", modelID: "gpt-6-astra", variant: "high" };
  const luna = { providerID: "openai", modelID: "gpt-5.6-luna", variant: "max" };
  const config = {
    model: "openai/gpt-6-astra",
    command: {
      commit: { template: "Commit this session's changes", model: "openai/gpt-5.6-luna#max" },
      plain: { template: "No thinking override", model: "openai/gpt-5.6-luna" },
      review: { template: "No model override" },
    },
  } satisfies Config;
  await hooks.config(config);
  expect(config.model).toBe("openai/gpt-6-astra");
  expect(config.command.commit).toEqual({ template: "Commit this session's changes" });
  expect(config.command.plain.model).toBeUndefined();
  const command = (sessionID: string, name = "commit") =>
    hooks["command.execute.before"](
      { sessionID, command: name, arguments: "" },
      { parts: [] },
    );
  const chat = async (sessionID: string, plugin = hooks) => {
    const output: Parameters<typeof hooks["chat.message"]>[1] = {
      message: {
        id: "msg_test",
        sessionID,
        role: "user",
        time: { created: 0 },
        agent: "build",
        model: astra,
      },
      parts: [],
    };
    await plugin["chat.message"]({ sessionID, model: astra }, output);
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
  await command("a");
  await command("a", "review");
  expect(await chat("a")).toBe(astra);

  await command("a");
  expect(await chat("a", await commandModel())).toBe(astra);
  expect(await chat("a")).toEqual(luna);
});

test("rejects malformed command models", async () => {
  const hooks = await commandModel();
  for (const model of ["luna", "openai/", "/luna", "openai/luna#", "openai/luna#max#high"]) {
    await expect(hooks.config({ command: { commit: { template: "Commit", model } } }))
      .rejects.toThrow("expected provider/model[#variant]");
  }
});
