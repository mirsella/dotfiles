import type { Plugin } from "@opencode-ai/plugin";

type NotifyOptions = {
	title: string;
	message: string;
	urgency?: "low" | "normal" | "critical";
	icon?: string;
	category?: string;
	expireTime?: number;
};

type GlobalNotificationState = {
	activeIdleSessions: Set<string>;
	recentAborts: Map<string, number>;
	recentNotifications: Map<string, number>;
	notificationQueue: Promise<void>;
};

const WINDOWS = {
	abort: 5000,
	idle: 1000,
	notification: 2000,
} as const;

const globalState = globalThis as typeof globalThis & {
	__opencodeNotificationState?: GlobalNotificationState;
};

const state = (() => {
	if (!globalState.__opencodeNotificationState) {
		// Opencode may evaluate the same plugin multiple times.
		globalState.__opencodeNotificationState = {
			activeIdleSessions: new Set<string>(),
			recentAborts: new Map<string, number>(),
			recentNotifications: new Map<string, number>(),
			notificationQueue: Promise.resolve(),
		};
	}

	return globalState.__opencodeNotificationState;
})();

const isRecent = (
	map: Map<string, number>,
	key: string,
	window: number,
): boolean => {
	const now = Date.now();

	for (const [entry, timestamp] of map) {
		if (now - timestamp >= window) {
			map.delete(entry);
		}
	}

	const last = map.get(key);
	return last !== undefined && now - last < window;
};

const remember = (map: Map<string, number>, key: string): void => {
	map.set(key, Date.now());
};

const queueNotification = async (callback: () => Promise<void>): Promise<void> => {
	state.notificationQueue = state.notificationQueue.catch(() => {}).then(callback);
	await state.notificationQueue;
};

const errorMessage = (error: { name: string; data: Record<string, unknown> }): string => {
	switch (error.name) {
		case "APIError":
			return `API Error: ${error.data.message}`;
		case "ProviderAuthError":
			return `Auth Error: ${error.data.message}`;
		case "MessageOutputLengthError":
			return "Output length exceeded";
		case "UnknownError":
			return String(error.data.message ?? "Unknown error");
		default:
			return "Unknown error";
	}
};

export const NotificationPlugin: Plugin = async ({ client, $ }) => {
	const sendDesktop = async ({
		title,
		message,
		urgency = "normal",
		icon = "starred",
		category = "opencode",
		expireTime = 6000,
	}: NotifyOptions): Promise<void> => {
		const args = [
			"notify-send",
			`OC ${title}`,
			message,
			`--urgency=${urgency}`,
			`--icon=${icon}`,
			`--category=opencode.${category}`,
			"--app-name=OpenCode",
			"--expire-time",
			expireTime.toString(),
		];

		await queueNotification(async () => {
			await $`${args}`.quiet().nothrow();
		});
	};

	const notify = async (opts: NotifyOptions): Promise<void> => {
		const key = `${opts.category}:${opts.title}:${opts.message}`;
		if (isRecent(state.recentNotifications, key, WINDOWS.notification)) {
			return;
		}
		remember(state.recentNotifications, key);

		const message = `OC ${opts.title} ${opts.message}`;
		const phoneConnected = await (async (): Promise<boolean> => {
			try {
				const result = await $`kdeconnect-cli --list-devices`.quiet().nothrow();
				return String(result.stdout || result.text || result)
					.split("\n")
					.some((line) => line.includes("phone") && line.includes("reachable"));
			} catch {
				return false;
			}
		})();

		const telegramError = await (async (): Promise<string | null> => {
			const { TgId: tgId, TgToken: tgToken } = process.env;
			if (!tgId || !tgToken) {
				return "Telegram env vars (TgId/TgToken) not set";
			}

			try {
				const response = await fetch(
					`https://api.telegram.org/bot${tgToken}/sendMessage`,
					{
						method: "POST",
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify({ chat_id: tgId, text: message }),
						signal: AbortSignal.timeout(2000),
					},
				);

				return response.ok ? null : `Telegram API error: ${response.status}`;
			} catch (error) {
				return `Telegram fetch failed: ${error instanceof Error ? error.message : "Unknown error"}`;
			}
		})();

		if (phoneConnected && telegramError === null) {
			return;
		}

		if (telegramError) {
			await sendDesktop({
				title: "❌",
				message: telegramError,
				urgency: "critical",
				icon: "dialog-error",
				category: "error",
				expireTime: 10000,
			});
		}

		await sendDesktop(opts);
	};

	return {
		event: async ({ event }) => {
			switch (event.type) {
				case "question.asked": {
					// @ts-ignore - EventQuestionAsk not in installed SDK types yet
					const question = event.properties.questions[0]?.question || "Question pending";
					await notify({
						title: "❓",
						message: question,
						urgency: "critical",
						icon: "dialog-information",
						category: "question",
						expireTime: 8000,
					});
					return;
				}

				case "permission.asked": {
					// @ts-ignore - EventPermissionAsk not in installed SDK types yet
					await notify({
						title: "🔐",
						message: event.properties.title || "Permission Required",
						urgency: "critical",
						icon: "dialog-question",
						category: "permission",
						expireTime: 5000,
					});
					return;
				}

				case "session.error": {
					const { error, sessionID } = event.properties;
					if (!error) {
						return;
					}
					if (error.name === "MessageAbortedError") {
						if (sessionID) {
							remember(state.recentAborts, sessionID);
						}
						return;
					}

					await notify({
						title: "❌",
						message: errorMessage(error),
						urgency: "critical",
						icon: "dialog-error",
						category: "error",
						expireTime: 10000,
					});
					return;
				}

				case "session.idle":
				case "session.status": {
					if (
						event.type === "session.status" &&
						event.properties.status.type !== "idle"
					) {
						return;
					}

					const { sessionID } = event.properties;
					if (
						state.activeIdleSessions.has(sessionID) ||
						isRecent(state.recentAborts, sessionID, WINDOWS.abort)
					) {
						return;
					}

					state.activeIdleSessions.add(sessionID);

					try {
						let title = "Session";
						try {
							const session = await client.session.get({ path: { id: sessionID } });
							if (session.data?.parentID) {
								return;
							}
							title = session.data?.title || title;
						} catch {}

						await notify({
							title: "✅",
							message: title,
							urgency: "normal",
							icon: "starred",
							category: "completed",
							expireTime: 6000,
						});
					} finally {
						setTimeout(() => {
							state.activeIdleSessions.delete(sessionID);
						}, WINDOWS.idle);
					}
					return;
				}

				default:
					return;
			}
		},
	};
};
