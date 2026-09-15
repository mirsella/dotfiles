import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { asModelSpec, type ModelSpec } from "./model-spec";

export type ForcedMode = "go" | "codex";
export type Mode = "auto" | ForcedMode;

// Defaults for the forced modes until overridden from the /subagents dialog.
export const MODE_MODELS: Record<ForcedMode, ModelSpec> = {
  go: { providerID: "opencode-go", modelID: "deepseek-v4.1-flash", variant: "max" },
  codex: { providerID: "openai", modelID: "gpt-5.6-luna", variant: "max" },
};

type Scope = {
  mode?: Mode;
  models?: Partial<Record<ForcedMode, ModelSpec>>;
};

export type State = {
  global?: Scope;
  sessions?: Record<string, Scope>;
};

export const stateFile = (dir?: string) =>
  process.env.OPENCODE_SUBAGENT_MODE_FILE ??
  join(dir ?? join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "opencode"), "subagent-mode.json");

const sanitizeScope = (value: unknown, dropped: { any: boolean }): Scope | undefined => {
  if (typeof value !== "object" || value === null) {
    dropped.any = true;
    return undefined;
  }
  const { mode, models } = value as Record<string, unknown>;
  const scope: Scope = {};
  if (mode !== undefined) {
    if (mode === "auto" || mode === "go" || mode === "codex") scope.mode = mode;
    else dropped.any = true;
  }
  if (models !== undefined) {
    if (typeof models !== "object" || models === null) dropped.any = true;
    else {
      for (const forced of ["go", "codex"] as const) {
        const value = (models as Record<string, unknown>)[forced];
        if (value === undefined) continue;
        const spec = asModelSpec(value);
        if (spec === undefined) dropped.any = true;
        else (scope.models ??= {})[forced] = spec;
      }
    }
  }
  return scope;
};

export const readState = (file = stateFile()): State => {
  let text: string;
  try {
    text = readFileSync(file, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
    throw error;
  }
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (error) {
    console.warn(
      `[subagent-mode] ignoring ${file}: ${error instanceof Error ? error.message : String(error)}`,
    );
    return {};
  }
  const dropped = { any: false };
  const state: State = {};
  if (typeof raw !== "object" || raw === null) {
    dropped.any = true;
  } else {
    const { global, sessions } = raw as Record<string, unknown>;
    if (global !== undefined) {
      const scope = sanitizeScope(global, dropped);
      if (scope !== undefined) state.global = scope;
    }
    if (sessions !== undefined) {
      if (typeof sessions !== "object" || sessions === null) dropped.any = true;
      else {
        for (const [sessionID, value] of Object.entries(sessions)) {
          const scope = sanitizeScope(value, dropped);
          if (scope === undefined) continue;
          state.sessions ??= {};
          state.sessions[sessionID] = scope;
        }
      }
    }
  }
  // A hand-edited or stale file must not break delegation, but the fallback is
  // worth knowing about.
  if (dropped.any) console.warn(`[subagent-mode] ignoring invalid fields in ${file}`);
  return state;
};

export const writeState = (state: State, file = stateFile()) => {
  mkdirSync(dirname(file), { recursive: true });
  const temporary = `${file}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(state, null, 2)}\n`);
  renameSync(temporary, file);
};

export const resolveScope = (state: State, sessionID?: string) => {
  const session = sessionID === undefined ? undefined : state.sessions?.[sessionID];
  const global = state.global;
  const mode: Mode = session?.mode ?? global?.mode ?? "auto";
  return {
    mode,
    models: {
      go: session?.models?.go ?? global?.models?.go ?? MODE_MODELS.go,
      codex: session?.models?.codex ?? global?.models?.codex ?? MODE_MODELS.codex,
    },
  };
};
