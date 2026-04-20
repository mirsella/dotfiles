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
	recentlyAbortedSessions: Map<string, number>;
	recentNotifications: Map<string, number>;
	notificationQueue: Promise<void>;
};

const ABORT_SUPPRESSION_WINDOW_MS = 5000;
const IDLE_DEDUPE_WINDOW_MS = 1000;
const NOTIFICATION_DEDUPE_WINDOW_MS = 2000;

const sharedGlobal = globalThis as typeof globalThis & {
	__opencodeNotificationState?: GlobalNotificationState;
};

if (!sharedGlobal.__opencodeNotificationState) {
	// Opencode can load the same plugin from multiple config paths.
	sharedGlobal.__opencodeNotificationState = {
		activeIdleSessions: new Set<string>(),
		recentlyAbortedSessions: new Map<string, number>(),
		recentNotifications: new Map<string, number>(),
		notificationQueue: Promise.resolve(),
	};
}

const sharedState = sharedGlobal.__opencodeNotificationState;

const markSessionAborted = (sessionId?: string): void => {
	if (!sessionId) {
		return;
	}

	sharedState.recentlyAbortedSessions.set(sessionId, Date.now());
};

const wasSessionRecentlyAborted = (sessionId: string): boolean => {
	const now = Date.now();

	for (const [recentSessionId, timestamp] of sharedState.recentlyAbortedSessions) {
		if (now - timestamp >= ABORT_SUPPRESSION_WINDOW_MS) {
			sharedState.recentlyAbortedSessions.delete(recentSessionId);
		}
	}

	return sharedState.recentlyAbortedSessions.has(sessionId);
};

const shouldSkipDuplicateNotification = (key: string): boolean => {
	const now = Date.now();

	for (const [recentKey, timestamp] of sharedState.recentNotifications) {
		if (now - timestamp >= NOTIFICATION_DEDUPE_WINDOW_MS) {
			sharedState.recentNotifications.delete(recentKey);
		}
	}

	const previousTimestamp = sharedState.recentNotifications.get(key);
	if (
		previousTimestamp !== undefined &&
		now - previousTimestamp < NOTIFICATION_DEDUPE_WINDOW_MS
	) {
		return true;
	}

	sharedState.recentNotifications.set(key, now);
	return false;
};

const queueDesktopNotification = async (
	callback: () => Promise<void>,
): Promise<void> => {
	sharedState.notificationQueue = sharedState.notificationQueue
		.catch(() => {})
		.then(callback);
	await sharedState.notificationQueue;
};

export const NotificationPlugin: Plugin = async ({
	project,
	client,
	$,
	directory,
	worktree,
}) => {
	const isPhoneConnected = async (): Promise<boolean> => {
		try {
			const result = await $`kdeconnect-cli --list-devices`.quiet().nothrow();
			const stdout = String(result.stdout || result.text || result);
			return stdout
				.split("\n")
				.some((line) => line.includes("phone") && line.includes("reachable"));
		} catch {
			return false;
		}
	};

	const sendTelegram = async (message: string): Promise<string | null> => {
		const tgId = process.env.TgId;
		const tgToken = process.env.TgToken;

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
			if (!response.ok) {
				return `Telegram API error: ${response.status}`;
			}
			return null;
		} catch (error) {
			return `Telegram fetch failed: ${error instanceof Error ? error.message : "Unknown error"}`;
		}
	};

	const sendDesktopNotification = async (
		opts: NotifyOptions,
	): Promise<void> => {
		const {
			title,
			message,
			urgency = "normal",
			icon = "starred",
			category = "opencode",
			expireTime = 6000,
		} = opts;
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
		await queueDesktopNotification(async () => {
			await $`${args}`.quiet().nothrow();
		});
	};

	const notify = async (opts: NotifyOptions): Promise<void> => {
		const notificationKey = `${opts.category}:${opts.title}:${opts.message}`;
		if (shouldSkipDuplicateNotification(notificationKey)) {
			return;
		}

		const message = `OC ${opts.title} ${opts.message}`;

		const phoneConnected = await isPhoneConnected();
		const telegramError = await sendTelegram(message);

		const shouldFallback = !phoneConnected || telegramError !== null;
		if (!shouldFallback) return;

		if (telegramError) {
			await sendDesktopNotification({
				title: "❌",
				message: telegramError,
				urgency: "critical",
				icon: "dialog-error",
				category: "error",
				expireTime: 10000,
			});
		}

		await sendDesktopNotification(opts);
	};

	return {
		event: async ({ event }) => {
			// @ts-ignore - EventQuestionAsk not in installed SDK types yet
			if (event.type === "question.asked") {
				// @ts-ignore
				const questions = event.properties.questions;
				const q = questions[0];
				const questionText = q?.question || "Question pending";
				await notify({
					title: "❓",
					message: questionText,
					urgency: "critical",
					icon: "dialog-information",
					category: "question",
					expireTime: 8000,
				});
				return;
			}

			// @ts-ignore - EventPermissionAsk not in installed SDK types yet
			if (event.type === "permission.asked") {
				// @ts-ignore
				const req = event.properties;
				const title = req.title || "Permission Required";
				await notify({
					title: "🔐",
					message: title,
					urgency: "critical",
					icon: "dialog-question",
					category: "permission",
					expireTime: 5000,
				});
				return;
			}

			if (event.type === "session.error") {
				const { error, sessionID } = event.properties;
				if (!error) return;

				if (error.name === "MessageAbortedError") {
					markSessionAborted(sessionID);
					return;
				}

				let message = "Unknown error";
				if (error.name === "APIError") {
					message = `API Error: ${error.data.message}`;
				} else if (error.name === "ProviderAuthError") {
					message = `Auth Error: ${error.data.message}`;
				} else if (error.name === "MessageOutputLengthError") {
					message = "Output length exceeded";
				} else if (error.name === "UnknownError") {
					message = error.data.message;
				}

				await notify({
					title: "❌",
					message,
					urgency: "critical",
					icon: "dialog-error",
					category: "error",
					expireTime: 10000,
				});
				return;
			}

			const isIdleEvent =
				event.type === "session.idle" ||
				(event.type === "session.status" &&
					event.properties.status.type === "idle");
			if (!isIdleEvent) {
				return;
			}

			const sessionId = event.properties.sessionID;

			if (sharedState.activeIdleSessions.has(sessionId)) {
				return;
			}
			if (wasSessionRecentlyAborted(sessionId)) {
				return;
			}
			sharedState.activeIdleSessions.add(sessionId);

			try {
				let title = "Session";
				try {
					const session = await client.session.get({
						path: { id: sessionId },
					});
					if (session.data?.parentID) {
						return;
					}
					title = session.data?.title || "Session";
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
					sharedState.activeIdleSessions.delete(sessionId);
				}, IDLE_DEDUPE_WINDOW_MS);
			}
		},
	};
};
