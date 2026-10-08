import type { KeyEvent, TuiPluginModule } from "@opencode-ai/plugin/tui";

export default {
	id: "local.suppress-ctrl-u",
	async tui(api) {
		const suppressCtrlU = (event: KeyEvent) => {
			if (event.name !== "u" || !event.ctrl || !api.renderer.currentFocusedEditor?.plainText) return;
			event.preventDefault();
			event.stopPropagation();
		};

		api.renderer.keyInput.prependListener("keypress", suppressCtrlU);
		api.lifecycle.onDispose(() => api.renderer.keyInput.off("keypress", suppressCtrlU));
	},
} satisfies TuiPluginModule;
