import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { asModelSpec, type ModelSpec } from "./model-spec";

export type ForcedMode = "go" | "codex";
export type Mode = "auto" | ForcedMode;

// Defaults for the forced modes until overridden from the /subagents dialog.
export const MODE_MODELS: Record<ForcedMode, ModelSpec> = {
  go: {
    providerID: "opencode-go",
    modelID: "muse-spark-1.3-contributor",
    variant: "xhigh",
  },
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
  join(
    dir ??
      join(
        process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"),
        "opencode",
      ),
    "subagent-mode.json",
  );

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null;

const sanitizeScope = (value: unknown): { scope?: Scope; invalid: boolean } => {
  if (!isRecord(value)) return { invalid: true };
  const { mode, models } = value;
  const scope: Scope = {};
  let invalid = false;
  if (mode !== undefined) {
    if (mode === "auto" || mode === "go" || mode === "codex") scope.mode = mode;
    else invalid = true;
  }
  if (models !== undefined) {
    if (!isRecord(models)) invalid = true;
    else {
      for (const forced of ["go", "codex"] as const) {
        const value = models[forced];
        if (value === undefined) continue;
        const spec = asModelSpec(value);
        if (spec === undefined) invalid = true;
        else (scope.models ??= {})[forced] = spec;
      }
    }
  }
  return { scope, invalid };
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
  const state: State = {};
  let invalid = !isRecord(raw);
  if (isRecord(raw)) {
    const { global, sessions } = raw;
    if (global !== undefined) {
      const result = sanitizeScope(global);
      invalid ||= result.invalid;
      if (result.scope !== undefined) state.global = result.scope;
    }
    if (sessions !== undefined) {
      if (!isRecord(sessions)) invalid = true;
      else {
        for (const [sessionID, value] of Object.entries(sessions)) {
          const result = sanitizeScope(value);
          invalid ||= result.invalid;
          if (result.scope === undefined) continue;
          state.sessions ??= {};
          state.sessions[sessionID] = result.scope;
        }
      }
    }
  }
  // A hand-edited or stale file must not break delegation, but the fallback is
  // worth knowing about.
  if (invalid)
    console.warn(`[subagent-mode] ignoring invalid fields in ${file}`);
  return state;
};

export const writeState = (state: State, file = stateFile()) => {
  mkdirSync(dirname(file), { recursive: true });
  const temporary = `${file}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(state, null, 2)}\n`);
  renameSync(temporary, file);
};

export const resolveScope = (state: State, sessionID?: string) => {
  const session =
    sessionID === undefined ? undefined : state.sessions?.[sessionID];
  const global = state.global;
  const mode: Mode = session?.mode ?? global?.mode ?? "auto";
  return {
    mode,
    models: {
      go: session?.models?.go ?? global?.models?.go ?? MODE_MODELS.go,
      codex:
        session?.models?.codex ?? global?.models?.codex ?? MODE_MODELS.codex,
    },
  };
};
