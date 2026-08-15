import type { KeyEvent, TuiPlugin, TuiPluginModule } from "@opencode-ai/plugin/tui";

const CTRL_U = "\x15";

const tui: TuiPlugin = async (api) => {
	const shouldSuppressCtrlU = () => (api.renderer.currentFocusedEditor?.plainText.length ?? 0) > 0;
	const rawHandler = (sequence: string) => sequence === CTRL_U && shouldSuppressCtrlU();
	const keyHandler = (event: KeyEvent) => {
		if (event.name !== "u" || !event.ctrl || !shouldSuppressCtrlU()) return;
		event.preventDefault();
		event.stopPropagation();
	};

	// Kitty keyboard input bypasses the legacy raw Ctrl+U byte, so both hooks are required.
	api.renderer.prependInputHandler(rawHandler);
	api.renderer.keyInput.on("keypress", keyHandler);
	api.lifecycle.onDispose(() => {
		api.renderer.removeInputHandler(rawHandler);
		api.renderer.keyInput.off("keypress", keyHandler);
	});
};

const plugin: TuiPluginModule & { id: string } = {
	id: "local.suppress-ctrl-u",
	tui,
};

export default plugin;
