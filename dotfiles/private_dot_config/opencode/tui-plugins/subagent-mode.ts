import type { TuiPluginModule } from "@opencode-ai/plugin/tui";
import { formatModel, parseModel } from "../lib/model-spec";
import {
  readState,
  resolveScope,
  stateFile,
  writeState,
  type ForcedMode,
  type Mode,
  type State,
} from "../lib/subagent-mode";

const autoDescription = "luna max; sol-fast on -fast; else inherit";

// Built-in pickers widen themselves on large terminals; mirror that.
const dialogSize = (width: number): "medium" | "large" | "xlarge" => {
  if (width >= 128) return "xlarge";
  if (width >= 96) return "large";
  return "medium";
};

export default {
  id: "local.subagent-mode",
  async tui(api) {
    const file = () => stateFile(api.state.path.state);
    const sessionID = () => {
      const route = api.route.current;
      return route.name === "session" && typeof route.params?.sessionID === "string"
        ? route.params.sessionID
        : undefined;
    };
    // Without a session ID the dialog edits the global default instead.
    const scopeOf = (state: State, id: string | undefined) =>
      id === undefined ? (state.global ??= {}) : ((state.sessions ??= {})[id] ??= {});
    const update = (mutate: (state: State) => void) => {
      const state = readState(file());
      mutate(state);
      writeState(state, file());
      return state;
    };
    const finish = (message: string) => {
      api.ui.dialog.clear();
      api.ui.toast({ message, variant: "success" });
    };
    const describe = (state: State, id: string | undefined, mode: Mode) =>
      mode === "auto" ? "auto" : `${mode} · ${formatModel(resolveScope(state, id).models[mode])}`;

    const promptModel = (id: string | undefined, mode: ForcedMode) => {
      const current = resolveScope(readState(file()), id).models[mode];
      api.ui.dialog.replace(() =>
        api.ui.DialogPrompt({
          title: `Custom ${mode} model`,
          placeholder: "provider/model[#variant]",
          value: formatModel(current),
          onConfirm: (text) => {
            const spec = parseModel(text);
            if (spec === undefined) {
              api.ui.toast({
                message: `Expected provider/model[#variant], got "${text}"`,
                variant: "error",
              });
              return;
            }
            update((state) => {
              const scope = scopeOf(state, id);
              (scope.models ??= {})[mode] = spec;
              scope.mode = mode;
            });
            finish(`Subagents: ${mode} · ${formatModel(spec)}`);
          },
          onCancel: () => api.ui.dialog.clear(),
        }),
      );
      api.ui.dialog.setSize(dialogSize(api.renderer.width));
    };

    const select = (value: string, id: string | undefined) => {
      if (value === "go:model" || value === "codex:model") {
        promptModel(id, value === "go:model" ? "go" : "codex");
        return;
      }
      if (value === "reset") {
        update((state) => {
          if (id === undefined) {
            delete state.global;
            return;
          }
          if (state.sessions === undefined) return;
          delete state.sessions[id];
          if (Object.keys(state.sessions).length === 0) delete state.sessions;
        });
        finish(id === undefined ? "Subagents: global default reset" : "Subagents: session override cleared");
        return;
      }
      const mode = value as Mode;
      const state = update((state) => (scopeOf(state, id).mode = mode));
      finish(`Subagents: ${describe(state, id, mode)}`);
    };

    const open = () => {
      const id = sessionID();
      const state = readState(file());
      const resolved = resolveScope(state, id);
      const options = [
        { title: "auto", value: "auto", description: autoDescription },
        { title: "go", value: "go", description: formatModel(resolved.models.go) },
        { title: "codex", value: "codex", description: formatModel(resolved.models.codex) },
        {
          title: "Custom go model…",
          value: "go:model",
          description: "provider/model[#variant]",
        },
        {
          title: "Custom codex model…",
          value: "codex:model",
          description: "provider/model[#variant]",
        },
      ];
      if ((id === undefined ? state.global : state.sessions?.[id]) !== undefined) {
        options.push({
          title: id === undefined ? "Reset global default" : "Reset this session",
          value: "reset",
          description: "clear mode and model",
        });
      }
      api.ui.dialog.replace(() =>
        api.ui.DialogSelect({
          title: id === undefined ? "Subagent provider · global default" : "Subagent provider · this session",
          current: resolved.mode,
          options,
          onSelect: (option) => select(String(option.value), id),
        }),
      );
      api.ui.dialog.setSize(dialogSize(api.renderer.width));
    };

    if (api.command === undefined) {
      console.warn("[subagent-mode] this opencode build has no TUI command API; /subagents is unavailable");
      return;
    }
    api.command.register(() => [
      {
        title: "Subagent provider",
        value: "subagent.provider",
        slash: { name: "subagents" },
        onSelect: open,
      },
    ]);
  },
} satisfies TuiPluginModule;
