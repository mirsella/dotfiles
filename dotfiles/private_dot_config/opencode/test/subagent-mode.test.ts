import { expect, test } from "bun:test";
import { mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { MODE_MODELS, readState, resolveScope, stateFile, writeState, type State } from "../lib/subagent-mode";

const temporaryState = (): string => join(mkdtempSync(join(tmpdir(), "subagent-mode-")), "state.json");

test("defaults to the opencode data directory and honors the file override", () => {
  const previousData = process.env.XDG_DATA_HOME;
  const previousFile = process.env.OPENCODE_SUBAGENT_MODE_FILE;
  try {
    process.env.XDG_DATA_HOME = "/tmp/xdg-data";
    delete process.env.OPENCODE_SUBAGENT_MODE_FILE;
    expect(stateFile()).toBe("/tmp/xdg-data/opencode/subagent-mode.json");
    expect(stateFile("/explicit")).toBe("/explicit/subagent-mode.json");
    process.env.OPENCODE_SUBAGENT_MODE_FILE = "/tmp/custom.json";
    expect(stateFile("/ignored")).toBe("/tmp/custom.json");
  } finally {
    if (previousData === undefined) delete process.env.XDG_DATA_HOME;
    else process.env.XDG_DATA_HOME = previousData;
    if (previousFile === undefined) delete process.env.OPENCODE_SUBAGENT_MODE_FILE;
    else process.env.OPENCODE_SUBAGENT_MODE_FILE = previousFile;
  }
});

test("round-trips state without leaving temporary files", () => {
  const file = temporaryState();
  const state: State = {
    global: { mode: "codex", models: { go: { providerID: "opencode-go", modelID: "kimi-k3" } } },
    sessions: { one: { mode: "go" }, two: {} },
  };
  writeState(state, file);
  expect(readState(file)).toEqual(state);
  expect(readdirSync(join(file, ".."))).toEqual(["state.json"]);
});

test("missing state files resolve to auto", () => {
  const file = temporaryState();
  expect(readState(file)).toEqual({});
  expect(resolveScope(readState(file), "one")).toMatchObject({ mode: "auto" });
});

test("corrupt state files degrade to defaults", () => {
  const file = temporaryState();
  writeFileSync(file, "{not json");
  expect(readState(file)).toEqual({});
  writeFileSync(file, JSON.stringify({ global: 3, sessions: [] }));
  expect(readState(file)).toEqual({});
});

test("invalid fields are dropped without discarding the scope", () => {
  const file = temporaryState();
  writeFileSync(
    file,
    JSON.stringify({
      global: { mode: "nope", models: { go: { providerID: "", modelID: "x" }, codex: { providerID: "openai", modelID: "gpt-6-luna", variant: 7 } } },
      sessions: { one: { mode: "go", models: { go: "deepseek" } }, two: { mode: "codex" } },
    }),
  );
  expect(readState(file)).toEqual({
    global: {},
    sessions: { one: { mode: "go" }, two: { mode: "codex" } },
  });
});

test("session scope overrides the global scope, which overrides the defaults", () => {
  const state: State = {
    global: { mode: "go", models: { codex: { providerID: "openai", modelID: "gpt-5.6-sol", variant: "high" } } },
    sessions: { one: { mode: "codex", models: { codex: { providerID: "openai", modelID: "gpt-6-astra" } } } },
  };
  const session = resolveScope(state, "one");
  expect(session.mode).toBe("codex");
  expect(session.models.go).toEqual(MODE_MODELS.go);
  expect(session.models.codex).toEqual({ providerID: "openai", modelID: "gpt-6-astra" });
  const global = resolveScope(state, "two");
  expect(global.mode).toBe("go");
  expect(global.models.codex).toEqual({ providerID: "openai", modelID: "gpt-5.6-sol", variant: "high" });
  const unset = resolveScope({}, "one");
  expect(unset.mode).toBe("auto");
  expect(unset.models).toEqual(MODE_MODELS);
});
