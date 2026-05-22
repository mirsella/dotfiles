import type { Plugin } from "@opencode-ai/plugin";
import type { AssistantMessage, Message, UserMessage } from "@opencode-ai/sdk";

const RETRY_PROMPT = "continue";
const RETRY_DELAY_MS = 10_000;
const MESSAGE_FETCH_LIMIT = 24;

type Client = Parameters<Plugin>[0]["client"];
type Log = (
	level: "debug" | "info" | "warn" | "error",
	message: string,
	extra?: Record<string, unknown>,
) => Promise<void>;

type MessageEntry = {
	info: Message;
	parts: Array<{ type: string; text?: string }>;
};

type RetryState = {
	token?: symbol;
	timer?: ReturnType<typeof setTimeout>;
};

const globalState = globalThis as typeof globalThis & {
	__opencodeRetryDatabaseLocked?: Map<string, RetryState>;
};

if (!globalState.__opencodeRetryDatabaseLocked) {
	globalState.__opencodeRetryDatabaseLocked = new Map<string, RetryState>();
}
const states = globalState.__opencodeRetryDatabaseLocked;

const stateFor = (sessionID: string): RetryState => {
	const existing = states.get(sessionID);
	if (existing) return existing;

	const created: RetryState = {};
	states.set(sessionID, created);
	return created;
};

const isObject = (value: unknown): value is Record<string, unknown> =>
	typeof value === "object" && value !== null;

const isDatabaseLockedError = (
	error: AssistantMessage["error"] | undefined,
): boolean => {
	if (!error) return false;

	const parts: string[] = [];
	if ("name" in error && typeof error.name === "string") parts.push(error.name);
	if ("data" in error && isObject(error.data)) {
		const data = error.data as Record<string, unknown>;
		for (const key of ["message", "code", "type", "responseBody"] as const) {
			const value = data[key];
			if (typeof value === "string") parts.push(value);
		}
	}

	const text = parts.join("\n").toLowerCase();
	return (
		text.includes("database is locked") ||
		text.includes("database locked") ||
		text.includes("sqlite_busy") ||
		text.includes("sqlite_locked")
	);
};

const messages = async (client: Client, sessionID: string) => {
	const response = await client.session.messages({
		path: { id: sessionID },
		query: { limit: MESSAGE_FETCH_LIMIT },
	});
	return response.data ?? [];
};

const messageEntry = async (
	client: Client,
	sessionID: string,
	messageID: string,
): Promise<MessageEntry | undefined> => {
	const response = await client.session.message({
		path: { id: sessionID, messageID },
	});
	return response.data as MessageEntry | undefined;
};

const latestEntry = (
	entries: Awaited<ReturnType<typeof messages>>,
): MessageEntry | undefined =>
	[...entries].sort((a, b) => b.info.time.created - a.info.time.created)[0];

const parentUserFor = async (
	client: Client,
	sessionID: string,
	entry: MessageEntry | undefined,
): Promise<UserMessage | undefined> => {
	if (!entry) return undefined;
	if (entry.info.role === "user") return entry.info;
	if (entry.info.role !== "assistant") return undefined;

	const parent = await messageEntry(
		client,
		sessionID,
		entry.info.parentID,
	).catch(() => undefined);
	return parent?.info.role === "user" ? parent.info : undefined;
};

export const RetryDatabaseLockedPlugin: Plugin = async ({ client }) => {
	const log: Log = async (level, message, extra) => {
		await client.app
			.log({
				body: { service: "retry-database-locked", level, message, extra },
			})
			.catch(() => undefined);
	};

	const toast = async (
		message: string,
		variant: "info" | "warning" | "error" = "warning",
	) => {
		await client.tui
			.showToast({ body: { message, variant, duration: 2500 } })
			.catch(() => undefined);
	};

	return {
		event: async ({ event }) => {
			if (event.type !== "session.error") return;

			const { sessionID, error } = event.properties;
			if (!sessionID || !isDatabaseLockedError(error)) return;

			const state = stateFor(sessionID);
			const entry = latestEntry(await messages(client, sessionID));
			const messageID = entry?.info.id;
			const token = Symbol(messageID ?? sessionID);

			if (state.timer) {
				clearTimeout(state.timer);
				state.timer = undefined;
			}
			state.token = token;

			const finish = () => {
				if (state.token !== token) return;
				state.token = undefined;
				if (state.timer) {
					clearTimeout(state.timer);
					state.timer = undefined;
				}
			};

			const retry = async () => {
				if (state.token !== token) return;

				try {
					if (
						entry &&
						(await messages(client, sessionID)).some(
							(candidate) =>
								candidate.info.time.created > entry.info.time.created,
						)
					) {
						await log(
							"info",
							"skipped database locked retry because the session moved forward",
							{ sessionID, messageID },
						);
						finish();
						return;
					}

					const parentUser = await parentUserFor(client, sessionID, entry);
					await client.session.promptAsync({
						path: { id: sessionID },
						body: {
							agent: parentUser?.agent,
							model: parentUser?.model,
							parts: [{ type: "text", text: RETRY_PROMPT }],
						},
					});

					await log("info", "sent continue after database locked error", {
						sessionID,
						messageID,
						agent: parentUser?.agent,
					});
					finish();
				} catch (error) {
					const message =
						error instanceof Error ? error.message : String(error);
					await log("error", "failed to send continue after database locked", {
						sessionID,
						messageID,
						error: message,
						nextDelayMs: RETRY_DELAY_MS,
					});
					await toast(
						`Database locked retry failed; retrying in 10s: ${message}`,
						"error",
					);
					if (state.token === token)
						state.timer = setTimeout(retry, RETRY_DELAY_MS);
				}
			};

			await toast("Database locked; retrying in 10s", "warning");
			await log("info", "scheduled database locked retry", {
				sessionID,
				messageID,
				delayMs: RETRY_DELAY_MS,
			});
			state.timer = setTimeout(retry, RETRY_DELAY_MS);
		},
	};
};
