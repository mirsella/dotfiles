import type { Plugin } from "@opencode-ai/plugin";
import type { AssistantMessage, Message, UserMessage } from "@opencode-ai/sdk";

const RETRY_PROMPT = "continue";
const QUICK_FAILURE_WINDOW_MS = 15_000;
const MESSAGE_POLL_ATTEMPTS = 5;
const MESSAGE_POLL_DELAY_MS = 200;
const MESSAGE_FETCH_LIMIT = 24;

type Client = Parameters<Plugin>[0]["client"];
type Log = (
	level: "debug" | "info" | "warn" | "error",
	message: string,
	extra?: Record<string, unknown>,
) => Promise<void>;

type MessageEntry = {
	info: Message;
	parts: Array<{
		type: string;
		text?: string;
		synthetic?: boolean;
		ignored?: boolean;
	}>;
};

type DeleteTransport = {
	delete?: (input: {
		url: string;
		path: { id: string; messageID: string };
	}) => Promise<unknown>;
};

type ClientWithTransport = {
	_client?: DeleteTransport;
};

type RetryState = {
	backoffStep: number;
	lastFailureAt: number;
	token?: symbol;
	timer?: ReturnType<typeof setTimeout>;
	lastMessageID?: string;
};

const globalState = globalThis as typeof globalThis & {
	__opencodeRetryServerErrors?: Map<string, RetryState>;
};

if (!globalState.__opencodeRetryServerErrors) {
	globalState.__opencodeRetryServerErrors = new Map<string, RetryState>();
}
const states = globalState.__opencodeRetryServerErrors;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const stateFor = (sessionID: string): RetryState => {
	const existing = states.get(sessionID);
	if (existing) return existing;

	const created: RetryState = { backoffStep: 0, lastFailureAt: 0 };
	states.set(sessionID, created);
	return created;
};

const isObject = (value: unknown): value is Record<string, unknown> =>
	typeof value === "object" && value !== null;

const providerPayload = (error: AssistantMessage["error"] | undefined) => {
	if (!error || !("data" in error) || !isObject(error.data)) return undefined;
	if (typeof error.data.message !== "string") return undefined;

	try {
		const parsed = JSON.parse(error.data.message);
		return isObject(parsed) && parsed.type === "error" && isObject(parsed.error)
			? { rawMessage: error.data.message, nested: parsed.error }
			: { rawMessage: error.data.message, nested: undefined };
	} catch {
		return { rawMessage: error.data.message, nested: undefined };
	}
};

const isProviderServerError = (
	error: AssistantMessage["error"] | undefined,
): boolean => {
	const payload = providerPayload(error);
	if (!payload) return false;

	const code =
		typeof payload.nested?.code === "string" ? payload.nested.code : "";
	const type =
		typeof payload.nested?.type === "string" ? payload.nested.type : "";
	const nestedMessage =
		typeof payload.nested?.message === "string" ? payload.nested.message : "";

	return (
		code === "server_error" ||
		code === "server_is_overloaded" ||
		type === "server_error" ||
		type === "service_unavailable_error" ||
		payload.rawMessage.includes(
			"An error occurred while processing your request",
		) ||
		payload.rawMessage.includes("Our servers are currently overloaded") ||
		nestedMessage.includes("An error occurred while processing your request") ||
		nestedMessage.includes("Our servers are currently overloaded")
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

const latestAssistantWithProviderError = (
	entries: Awaited<ReturnType<typeof messages>>,
): AssistantMessage | undefined => {
	const latest = entries
		.map((entry) => entry.info)
		.sort((a, b) => b.time.created - a.time.created)[0];
	return latest?.role === "assistant" && isProviderServerError(latest.error)
		? latest
		: undefined;
};

const pollFailingAssistant = async (client: Client, sessionID: string) => {
	for (let attempt = 0; attempt < MESSAGE_POLL_ATTEMPTS; attempt++) {
		const failingMessage = latestAssistantWithProviderError(
			await messages(client, sessionID),
		);
		if (failingMessage) return failingMessage;
		if (attempt + 1 < MESSAGE_POLL_ATTEMPTS) await wait(MESSAGE_POLL_DELAY_MS);
	}
};

const deleteMessage = async (
	client: Client,
	sessionID: string,
	messageID: string,
) => {
	const sessionWithTransport = client.session as typeof client.session &
		ClientWithTransport;
	const clientWithTransport = client as typeof client & ClientWithTransport;
	const transport = sessionWithTransport._client ?? clientWithTransport._client;
	if (!transport?.delete) {
		throw new Error("OpenCode SDK transport does not expose delete()");
	}

	await transport.delete({
		url: "/session/{id}/message/{messageID}",
		path: { id: sessionID, messageID },
	});
};

const isPlainContinueMessage = (
	entry: MessageEntry | undefined,
): entry is MessageEntry & { info: UserMessage } => {
	const part = entry?.parts[0];
	return (
		entry?.info.role === "user" &&
		entry.parts.length === 1 &&
		part?.type === "text" &&
		!part.synthetic &&
		!part.ignored &&
		(part.text ?? "").trim() === RETRY_PROMPT
	);
};

const logDeleteFailure = async (
	log: Log,
	message: string,
	error: unknown,
	extra: Record<string, unknown>,
) => {
	await log("warn", message, {
		...extra,
		error: error instanceof Error ? error.message : String(error),
	});
};

const deleteRetryChain = async (
	client: Client,
	log: Log,
	sessionID: string,
	failingMessage: AssistantMessage,
) => {
	const recentEntries = await messages(client, sessionID);

	await deleteMessage(client, sessionID, failingMessage.id).catch((error) =>
		logDeleteFailure(log, "failed to delete retryable error message", error, {
			sessionID,
			messageID: failingMessage.id,
		}),
	);

	const parentEntry = await messageEntry(
		client,
		sessionID,
		failingMessage.parentID,
	).catch(() => undefined);
	if (!isPlainContinueMessage(parentEntry)) return;

	const siblings = recentEntries
		.map((entry) => entry.info)
		.filter(
			(message): message is AssistantMessage =>
				message.role === "assistant" &&
				message.parentID === parentEntry.info.id &&
				message.id !== failingMessage.id,
		);
	if (siblings.some((message) => !message.error)) return;

	for (const sibling of siblings) {
		await deleteMessage(client, sessionID, sibling.id).catch((error) =>
			logDeleteFailure(log, "failed to delete sibling retry message", error, {
				sessionID,
				messageID: sibling.id,
				parentID: parentEntry.info.id,
			}),
		);
	}

	await deleteMessage(client, sessionID, parentEntry.info.id).catch((error) =>
		logDeleteFailure(
			log,
			"failed to delete retry parent continue message",
			error,
			{
				sessionID,
				messageID: parentEntry.info.id,
			},
		),
	);
};

const retryDelay = (state: RetryState) => {
	const now = Date.now();
	state.backoffStep =
		state.lastFailureAt && now - state.lastFailureAt < QUICK_FAILURE_WINDOW_MS
			? state.backoffStep + 1
			: 0;
	state.lastFailureAt = now;
	return state.backoffStep * 1000;
};

export const RetryServerErrorsPlugin: Plugin = async ({ client }) => {
	const log: Log = async (level, message, extra) => {
		await client.app
			.log({
				body: { service: "retry-server-errors", level, message, extra },
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
			if (!sessionID || !isProviderServerError(error)) return;

			const failingMessage = await pollFailingAssistant(client, sessionID);
			if (!failingMessage) return;

			const state = stateFor(sessionID);
			if (state.lastMessageID === failingMessage.id) return;
			state.lastMessageID = failingMessage.id;

			if (state.timer) {
				clearTimeout(state.timer);
				state.timer = undefined;
			}

			const delayMs = retryDelay(state);
			const token = Symbol(failingMessage.id);
			state.token = token;

			await deleteRetryChain(client, log, sessionID, failingMessage);
			if (delayMs > 0) await toast(`Retrying in ${delayMs / 1000}s`, "warning");

			state.timer = setTimeout(async () => {
				if (state.token !== token) return;

				try {
					if (
						(await messages(client, sessionID)).some(
							(entry) => entry.info.time.created > failingMessage.time.created,
						)
					) {
						await log(
							"info",
							"skipped retry because the session moved forward",
							{
								sessionID,
								messageID: failingMessage.id,
							},
						);
						return;
					}

					const parent = await messageEntry(
						client,
						sessionID,
						failingMessage.parentID,
					).catch(() => undefined);
					const parentUser =
						parent?.info.role === "user" ? parent.info : undefined;

					await client.session.promptAsync({
						path: { id: sessionID },
						body: {
							agent: parentUser?.agent,
							model: parentUser?.model,
							parts: [{ type: "text", text: RETRY_PROMPT }],
						},
					});

					await log("info", "scheduled retry for transient provider error", {
						sessionID,
						messageID: failingMessage.id,
						delayMs,
						agent: parentUser?.agent,
						modelID: failingMessage.modelID,
						providerID: failingMessage.providerID,
					});
				} catch (error) {
					const message =
						error instanceof Error ? error.message : String(error);
					await log("error", "failed to schedule retry", {
						sessionID,
						messageID: failingMessage.id,
						error: message,
					});
					await toast(`Retry failed: ${message}`, "error");
				} finally {
					if (state.token === token) {
						state.token = undefined;
						if (state.timer) {
							clearTimeout(state.timer);
							state.timer = undefined;
						}
					}
				}
			}, delayMs);
		},
	};
};
