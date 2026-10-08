import type { Plugin } from "@opencode-ai/plugin";
import { DoomLoopThresholdPlugin } from "../lib/doom-loop-threshold";

// NOTE: this module must keep exactly one runtime export (the default
// plugin factory). OpenCode's legacy plugin loader iterates every export
// of the module and throws "Plugin export is not a function" for the
// whole file if any export is not a plugin function, which silently
// disables the plugin. Implementation lives in ../lib/doom-loop-threshold.

// Threshold is hardcoded because auto-loaded plugins in plugins/ cannot
// receive options via the opencode.jsonc "plugin" tuple form.
const DoomLoopThresholdAutoPlugin: Plugin = async (input) =>
	DoomLoopThresholdPlugin(input, { threshold: 6 });

export default DoomLoopThresholdAutoPlugin;
