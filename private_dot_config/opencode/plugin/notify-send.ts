const processedSessions = new Set();
import type { Plugin } from "@opencode-ai/plugin";

type NotifyOptions = {
	title: string;
	message: string;
	urgency?: "low" | "normal" | "critical";
	icon?: string;
	category?: string;
	expireTime?: number;
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

	const sendDesktopNotification = async (opts: NotifyOptions): Promise<void> => {
		const {
			title,
			message,
			urgency = "normal",
			icon = "starred",
			category = "opencode",
			expireTime = 6000,
		} = opts;
		await $`notify-send "OpenCode ${title}" "${message}" --urgency=${urgency} --icon=${icon} --category=opencode.${category} --app-name=OpenCode --expire-time=${expireTime}`.nothrow();
	};

	const notify = async (opts: NotifyOptions): Promise<void> => {
		const message = `OpenCode ${opts.title}: ${opts.message}`;

		const phoneConnected = await isPhoneConnected();
		const telegramError = await sendTelegram(message);

		const shouldFallback = !phoneConnected || telegramError !== null;
		if (!shouldFallback) return;

		if (telegramError) {
			await sendDesktopNotification({
				title: "Error",
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
					title: "❓ Question",
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
					title: "🔐 Permission",
					message: title,
					urgency: "critical",
					icon: "dialog-question",
					category: "permission",
					expireTime: 5000,
				});
				return;
			}

			if (event.type === "session.error") {
				const { error } = event.properties;
				if (!error) return;

				let message = "Unknown error";
				if (error.name === "APIError") {
					message = `API Error: ${error.data.message}`;
				} else if (error.name === "ProviderAuthError") {
					message = `Auth Error: ${error.data.message}`;
				} else if (error.name === "MessageAbortedError") {
					message = `Aborted: ${error.data.message}`;
				} else if (error.name === "MessageOutputLengthError") {
					message = "Output length exceeded";
				} else if (error.name === "UnknownError") {
					message = error.data.message;
				}

				await notify({
					title: "❌ Error",
					message,
					urgency: "critical",
					icon: "dialog-error",
					category: "error",
					expireTime: 10000,
				});
				return;
			}

			if (
				event.type !== "session.status" ||
				event.properties.status.type !== "idle"
			) {
				return;
			}

			const sessionId = event.properties.sessionID;

			if (processedSessions.has(sessionId)) {
				return;
			}
			processedSessions.add(sessionId);

			setTimeout(() => {
				processedSessions.delete(sessionId);
			}, 100);

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
				title: "✅ Completed",
				message: title,
				urgency: "normal",
				icon: "starred",
				category: "completed",
				expireTime: 6000,
			});
		},
	};
};
