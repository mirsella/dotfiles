import type { Plugin } from "@opencode-ai/plugin";
import type { AssistantMessage, Message, UserMessage } from "@opencode-ai/sdk";

const RETRY_PROMPT = "continue";
const QUICK_FAILURE_WINDOW_MS = 15_000;
const DATABASE_LOCKED_RETRY_DELAY_MS = 1_000;
const MESSAGE_POLL_ATTEMPTS = 5;
const MESSAGE_POLL_DELAY_MS = 200;
const MESSAGE_FETCH_LIMIT = 24;

type Client = Parameters<Plugin>[0]["client"];
type RetryReason = "provider" | "database-locked";
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

type RetryableTail = {
	message: AssistantMessage;
	reason: RetryReason;
	shouldDelete: boolean;
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
	pendingToken?: symbol;
	pendingTimer?: ReturnType<typeof setTimeout>;
	lastHandledMessageID?: string;
};

const globalState = globalThis as typeof globalThis & {
	__opencodeRetryServerErrors?: Map<string, RetryState>;
};

if (!globalState.__opencodeRetryServerErrors) {
	globalState.__opencodeRetryServerErrors = new Map<string, RetryState>();
}
const states = globalState.__opencodeRetryServerErrors;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const getState = (sessionID: string): RetryState => {
	const existing = states.get(sessionID);
	if (existing) return existing;

	const created: RetryState = { backoffStep: 0, lastFailureAt: 0 };
	states.set(sessionID, created);
	return created;
};

const isObject = (value: unknown): value is Record<string, unknown> =>
	typeof value === "object" && value !== null;

const parseJson = (value: string): unknown => {
	try {
		return JSON.parse(value);
	} catch {
		return undefined;
	}
};

const unwrapProviderError = (message: string) => {
	const parsed = parseJson(message);
	if (!isObject(parsed) || parsed.type !== "error" || !isObject(parsed.error)) {
		return undefined;
	}

	return parsed.error;
};

const getErrorText = (error: AssistantMessage["error"] | undefined): string => {
	if (!error) return "";

	const parts: string[] = [];
	if ("name" in error && typeof error.name === "string") {
		parts.push(error.name);
	}
	if ("data" in error && isObject(error.data)) {
		const data = error.data as Record<string, unknown>;
		for (const key of ["message", "code", "type", "responseBody"] as const) {
			const value = data[key];
			if (typeof value === "string") parts.push(value);
		}
	}

	return parts.join("\n");
};

const classifyRetryableError = (
	error: AssistantMessage["error"] | undefined,
): RetryReason | undefined => {
	if (!error) return undefined;

	const text = getErrorText(error).toLowerCase();
	if (
		text.includes("database is locked") ||
		text.includes("database locked") ||
		text.includes("sqlite_busy") ||
		text.includes("sqlite_locked")
	) {
		return "database-locked";
	}

	const rawMessage =
		"data" in error &&
		isObject(error.data) &&
		typeof error.data.message === "string"
			? error.data.message
			: "";
	const nested = rawMessage ? unwrapProviderError(rawMessage) : undefined;
	const nestedCode = typeof nested?.code === "string" ? nested.code : "";
	const nestedType = typeof nested?.type === "string" ? nested.type : "";
	const nestedMessage =
		typeof nested?.message === "string" ? nested.message : "";

	if (
		nestedCode === "server_error" ||
		nestedCode === "server_is_overloaded" ||
		nestedType === "server_error" ||
		nestedType === "service_unavailable_error" ||
		rawMessage.includes("An error occurred while processing your request") ||
		rawMessage.includes("Our servers are currently overloaded") ||
		nestedMessage.includes("An error occurred while processing your request") ||
		nestedMessage.includes("Our servers are currently overloaded")
	) {
		return "provider";
	}

	return undefined;
};

const fetchMessages = async (client: Client, sessionID: string) => {
	const response = await client.session.messages({
		path: { id: sessionID },
		query: { limit: MESSAGE_FETCH_LIMIT },
	});
	return response.data ?? [];
};

const fetchMessageEntry = async (
	client: Client,
	sessionID: string,
	messageID: string,
): Promise<MessageEntry | undefined> => {
	const response = await client.session.message({
		path: { id: sessionID, messageID },
	});
	return response.data as MessageEntry | undefined;
};

const findRetryableTail = (
	entries: Awaited<ReturnType<typeof fetchMessages>>,
	eventReason: RetryReason,
): RetryableTail | undefined => {
	const tail = entries
		.map((entry) => entry.info)
		.sort((a, b) => b.time.created - a.time.created)[0];
	if (tail?.role !== "assistant") return undefined;

	const tailReason = classifyRetryableError(tail.error);
	if (tailReason)
		return { message: tail, reason: tailReason, shouldDelete: true };

	return eventReason === "database-locked"
		? { message: tail, reason: eventReason, shouldDelete: false }
		: undefined;
};

const pollRetryableTail = async (
	client: Client,
	sessionID: string,
	eventReason: RetryReason,
): Promise<RetryableTail | undefined> => {
	for (let attempt = 0; attempt < MESSAGE_POLL_ATTEMPTS; attempt++) {
		const retryableTail = findRetryableTail(
			await fetchMessages(client, sessionID),
			eventReason,
		);
		if (retryableTail) return retryableTail;

		if (attempt + 1 < MESSAGE_POLL_ATTEMPTS) {
			await wait(MESSAGE_POLL_DELAY_MS);
		}
	}

	return undefined;
};

const hasNewerMessages = async (
	client: Client,
	sessionID: string,
	since: number,
): Promise<boolean> => {
	const entries = await fetchMessages(client, sessionID);
	return entries.some((entry) => entry.info.time.created > since);
};

const deleteMessage = async (
	client: Client,
	sessionID: string,
	messageID: string,
): Promise<void> => {
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

const logDeleteFailure = async (
	log: Log,
	message: string,
	deleteError: unknown,
	extra: Record<string, unknown>,
) => {
	await log("warn", message, {
		...extra,
		error:
			deleteError instanceof Error ? deleteError.message : String(deleteError),
	});
};

const isPlainContinueMessage = (
	entry: MessageEntry | undefined,
): entry is MessageEntry & { info: UserMessage } => {
	if (!entry || entry.info.role !== "user") return false;
	if (entry.parts.length !== 1) return false;

	const [part] = entry.parts;
	return (
		part.type === "text" &&
		!part.synthetic &&
		!part.ignored &&
		(part.text ?? "").trim() === RETRY_PROMPT
	);
};

const removeRetryChainForParent = async (
	client: Client,
	sessionID: string,
	parentID: string,
	entries: MessageEntry[],
	skipMessageID: string | undefined,
	log: Log,
) => {
	const children = entries
		.map((entry) => entry.info)
		.filter(
			(message): message is AssistantMessage =>
				message.role === "assistant" &&
				message.parentID === parentID &&
				message.id !== skipMessageID,
		);
	if (children.some((child) => !child.error)) return false;

	for (const child of children) {
		await deleteMessage(client, sessionID, child.id).catch((deleteError) =>
			logDeleteFailure(
				log,
				"failed to delete sibling retry message",
				deleteError,
				{
					sessionID,
					messageID: child.id,
					parentID,
				},
			),
		);
	}

	await deleteMessage(client, sessionID, parentID).catch((deleteError) =>
		logDeleteFailure(
			log,
			"failed to delete retry parent continue message",
			deleteError,
			{ sessionID, messageID: parentID },
		),
	);

	return true;
};

const scheduleDelayMs = (
	state: RetryState,
	now: number,
	reason: RetryReason,
): number => {
	if (
		state.lastFailureAt &&
		now - state.lastFailureAt < QUICK_FAILURE_WINDOW_MS
	) {
		state.backoffStep += 1;
	} else {
		state.backoffStep = 0;
	}

	state.lastFailureAt = now;

	const baseDelayMs =
		reason === "database-locked" ? DATABASE_LOCKED_RETRY_DELAY_MS : 0;
	return baseDelayMs + state.backoffStep * 1000;
};

export const RetryServerErrorsPlugin: Plugin = async ({ client }) => {
	const log: Log = async (level, message, extra) => {
		await client.app
			.log({
				body: {
					service: "retry-server-errors",
					level,
					message,
					extra,
				},
			})
			.catch(() => undefined);
	};

	const toast = async (
		message: string,
		variant: "info" | "warning" | "error" = "warning",
	) => {
		await client.tui
			.showToast({
				body: { message, variant, duration: 2500 },
			})
			.catch(() => undefined);
	};

	return {
		event: async ({ event }) => {
			if (event.type !== "session.error") return;

			const { sessionID, error } = event.properties;
			const eventReason = classifyRetryableError(error);
			if (!sessionID || !eventReason) return;

			const retryableTail = await pollRetryableTail(
				client,
				sessionID,
				eventReason,
			);
			if (!retryableTail) return;
			const { message: failingMessage } = retryableTail;

			const state = getState(sessionID);
			if (state.lastHandledMessageID === failingMessage.id) return;
			state.lastHandledMessageID = failingMessage.id;

			if (state.pendingTimer) {
				clearTimeout(state.pendingTimer);
				state.pendingTimer = undefined;
			}

			const delayMs = scheduleDelayMs(state, Date.now(), retryableTail.reason);
			const token = Symbol(failingMessage.id);
			state.pendingToken = token;

			const recentEntries = await fetchMessages(client, sessionID);

			if (retryableTail.shouldDelete) {
				await deleteMessage(client, sessionID, failingMessage.id).catch(
					(deleteError) =>
						logDeleteFailure(
							log,
							"failed to delete retryable error message",
							deleteError,
							{ sessionID, messageID: failingMessage.id },
						),
				);

				const parentEntry = await fetchMessageEntry(
					client,
					sessionID,
					failingMessage.parentID,
				).catch(() => undefined);
				if (isPlainContinueMessage(parentEntry)) {
					await removeRetryChainForParent(
						client,
						sessionID,
						parentEntry.info.id,
						recentEntries,
						failingMessage.id,
						log,
					);
				}
			}

			if (delayMs > 0) {
				await toast(`Retrying in ${delayMs / 1000}s`, "warning");
			}

			state.pendingTimer = setTimeout(async () => {
				if (state.pendingToken !== token) return;

				try {
					if (
						await hasNewerMessages(
							client,
							sessionID,
							failingMessage.time.created,
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

					const parentEntry = await fetchMessageEntry(
						client,
						sessionID,
						failingMessage.parentID,
					).catch(() => undefined);
					const parentUserMessage =
						parentEntry?.info.role === "user" ? parentEntry.info : undefined;

					await client.session.promptAsync({
						path: { id: sessionID },
						body: {
							agent: parentUserMessage?.agent,
							model: parentUserMessage?.model,
							parts: [{ type: "text", text: RETRY_PROMPT }],
						},
					});

					await log("info", "scheduled retry for transient error", {
						sessionID,
						messageID: failingMessage.id,
						reason: retryableTail.reason,
						delayMs,
						agent: parentUserMessage?.agent,
						modelID: failingMessage.modelID,
						providerID: failingMessage.providerID,
					});
				} catch (retryError) {
					const message =
						retryError instanceof Error
							? retryError.message
							: String(retryError);
					await log("error", "failed to schedule retry", {
						sessionID,
						messageID: failingMessage.id,
						error: message,
					});
					await toast(`Retry failed: ${message}`, "error");
				} finally {
					if (state.pendingToken === token) {
						state.pendingToken = undefined;
					}
					if (state.pendingTimer) {
						clearTimeout(state.pendingTimer);
						state.pendingTimer = undefined;
					}
				}
			}, delayMs);
		},
	};
};
