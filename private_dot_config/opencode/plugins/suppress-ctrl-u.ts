import type { TuiPlugin, TuiPluginModule } from "@opencode-ai/plugin/tui";

const CTRL_U = "\x15";
const CTRL_C = "\x03";
const BACKSPACE = "\x7f";
const BRACKETED_PASTE_START = "\x1b[200~";
const BRACKETED_PASTE_END = "\x1b[201~";

type InputHandler = (sequence: string) => boolean;

type RendererInputApi = {
	currentFocusedRenderable?: FocusedRenderableLike | null;
	prependInputHandler?: (handler: InputHandler) => void;
	addInputHandler?: (handler: InputHandler) => void;
	removeInputHandler?: (handler: InputHandler) => void;
	keyInput?: {
		on?: (event: "keypress", handler: (event: KeyEventLike) => void) => void;
		off?: (event: "keypress", handler: (event: KeyEventLike) => void) => void;
		removeListener?: (
			event: "keypress",
			handler: (event: KeyEventLike) => void,
		) => void;
	};
};

type FocusedRenderableLike = {
	id?: unknown;
	focused?: boolean;
	plainText?: unknown;
	value?: unknown;
	text?: unknown;
	constructor?: { name?: string };
};

type KeyEventLike = {
	name?: string;
	ctrl?: boolean;
	preventDefault?: () => void;
	stopPropagation?: () => void;
};

const focusedInputText = (
	renderer: RendererInputApi,
	warnUnreadableInput: () => void,
): string | undefined => {
	const focused = renderer.currentFocusedRenderable;
	if (!focused?.focused) return undefined;

	const label = `${String(focused.id ?? "")} ${String(focused.constructor?.name ?? "")}`;
	if (!/input|textarea|prompt/i.test(label)) return undefined;

	for (const candidate of [focused.plainText, focused.value, focused.text]) {
		if (typeof candidate === "string") return candidate;
	}

	warnUnreadableInput();
	return undefined;
};

const printableText = (sequence: string): string => {
	if (sequence.startsWith(BRACKETED_PASTE_START) && sequence.endsWith(BRACKETED_PASTE_END)) {
		return sequence.slice(BRACKETED_PASTE_START.length, -BRACKETED_PASTE_END.length);
	}

	if (sequence.length !== 1) return "";
	if (sequence < " " || sequence === BACKSPACE || sequence === "\x1b") return "";
	return sequence;
};

const tui: TuiPlugin = async (api) => {
	const renderer = api.renderer as RendererInputApi;
	let homeInputFallback = "";
	let warnedUnreadableInput = false;

	const warnUnreadableInput = () => {
		if (warnedUnreadableInput) return;
		warnedUnreadableInput = true;
		console.warn("[suppress-ctrl-u] focused input exposes no readable text; using home-route fallback");
	};

	const shouldSuppressCtrlU = () => {
		const focusedText = focusedInputText(renderer, warnUnreadableInput);
		if (focusedText !== undefined) return focusedText.length > 0;

		return api.route.current.name === "home" && homeInputFallback.length > 0;
	};

	const trackHomeInputFallback = (sequence: string) => {
		if (api.route.current.name !== "home") return;

		if (sequence === CTRL_C || sequence === "\r" || sequence === "\n") {
			homeInputFallback = "";
			return;
		}

		if (sequence === BACKSPACE || sequence === "\b") {
			homeInputFallback = homeInputFallback.slice(0, -1);
			return;
		}

		homeInputFallback += printableText(sequence);
	};

	const rawHandler: InputHandler = (sequence) => {
		if (sequence === CTRL_U) return shouldSuppressCtrlU();

		trackHomeInputFallback(sequence);
		return false;
	};

	if (renderer.prependInputHandler) {
		renderer.prependInputHandler(rawHandler);
		api.lifecycle.onDispose(() => renderer.removeInputHandler?.(rawHandler));
		return;
	}

	if (renderer.addInputHandler) {
		console.warn("[suppress-ctrl-u] prependInputHandler unavailable; using addInputHandler fallback");
		renderer.addInputHandler(rawHandler);
		api.lifecycle.onDispose(() => renderer.removeInputHandler?.(rawHandler));
		return;
	}

	if (!renderer.keyInput?.on) {
		console.warn("[suppress-ctrl-u] renderer exposes no input hook; plugin disabled");
		return;
	}

	console.warn("[suppress-ctrl-u] raw input hooks unavailable; using keypress fallback");
	const keyHandler = (event: KeyEventLike) => {
		if (event.name !== "u" || !event.ctrl || !shouldSuppressCtrlU()) return;
		event.preventDefault?.();
		event.stopPropagation?.();
	};

	renderer.keyInput.on("keypress", keyHandler);
	api.lifecycle.onDispose(() => {
		renderer.keyInput?.off?.("keypress", keyHandler);
		renderer.keyInput?.removeListener?.("keypress", keyHandler);
	});
};

const plugin: TuiPluginModule & { id: string } = {
	id: "local.suppress-ctrl-u",
	tui,
};

export default plugin;
