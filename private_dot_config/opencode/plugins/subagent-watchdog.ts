import type { Plugin } from "@opencode-ai/plugin";
import type { Event as RuntimeEvent } from "@opencode-ai/sdk/v2";
import { homedir } from "node:os";
import { join } from "node:path";
import {
	RecoveryStore,
	SubagentWatchdog,
	createApi,
	errorText,
	loadConfig,
	type Log,
	type Notify,
} from "../lib/subagent-watchdog";

// NOTE: this module must keep exactly one runtime export (the default
// plugin factory). OpenCode's legacy plugin loader iterates every export
// of the module and throws "Plugin export is not a function" for the
// whole file if any export is not a plugin function, which silently
// disables the watchdog. Implementation lives in ../lib/subagent-watchdog.

const configPath = join(
	process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"),
	"opencode/subagent-watchdog.json",
);
const statePath = join(
	process.env.XDG_STATE_HOME ?? join(homedir(), ".local/state"),
	"opencode/subagent-watchdog.json",
);
const globals = globalThis as typeof globalThis & {
	__opencodeSubagentWatchdogStores?: Map<string, Promise<RecoveryStore>>;
};
const sharedStores =
	globals.__opencodeSubagentWatchdogStores ?? new Map<string, Promise<RecoveryStore>>();
globals.__opencodeSubagentWatchdogStores = sharedStores;

const SubagentWatchdogPlugin: Plugin = async ({ client, directory }) => {
	const log: Log = async (level, event, extra = {}) => {
		await client.app
			.log({
				query: { directory },
				body: {
					service: "subagent-watchdog",
					level,
					message: event,
					extra: { event, ...extra },
				},
			})
			.catch(() => undefined);
	};
	const notify: Notify = async (message, variant = "warning") => {
		await client.tui
			.showToast({
				query: { directory },
				body: { title: "Subagent watchdog", message, variant, duration: 6_000 },
			})
			.catch(() => undefined);
	};
	const loaded = await loadConfig(configPath);
	for (const warning of loaded.warnings)
		await log("warn", "watchdog.config.invalid", { warning });
	if (!loaded.config.enabled) {
		await log("info", "watchdog.disabled");
		return {};
	}

	let storePromise = sharedStores.get(statePath);
	if (!storePromise) {
		storePromise = RecoveryStore.open(statePath, Date.now(), (warning) =>
			log("error", "watchdog.state.invalid", { warning }),
		);
		sharedStores.set(statePath, storePromise);
	}
	const watchdog = new SubagentWatchdog(
		loaded.config,
		createApi(client, directory),
		await storePromise,
		log,
		notify,
	);
	await watchdog.start();
	await log("info", "watchdog.started", {
		mode: loaded.config.mode,
		suspectAfterMs: loaded.config.suspectAfterMs,
		recoverAfterMs: loaded.config.recoverAfterMs,
		toolRecoverAfterMs: loaded.config.toolRecoverAfterMs,
	});

	const guarded = async (operation: string, callback: () => Promise<void>) => {
		try {
			await callback();
		} catch (error) {
			await log("error", "watchdog.hook.failed", {
				operation,
				error: errorText(error),
			});
		}
	};
	return {
		dispose: async () => watchdog.stop(),
		event: async ({ event }) =>
			guarded("event", () => watchdog.handleEvent(event as unknown as RuntimeEvent)),
		"chat.message": async (input) => {
			watchdog.recordPromptContext(input.sessionID, {
				agent: input.agent,
				model: input.model,
				variant: input.variant,
			});
		},
		"tool.execute.before": async (input, output) =>
			guarded("tool.execute.before", () => watchdog.handleToolBefore(input, output)),
		"tool.execute.after": async (input, output) =>
			guarded("tool.execute.after", () => watchdog.handleToolAfter(input, output)),
	};
};

export default SubagentWatchdogPlugin;
