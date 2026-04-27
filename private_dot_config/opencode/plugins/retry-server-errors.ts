import type { Plugin } from "@opencode-ai/plugin";
import type { AssistantMessage, Message, UserMessage } from "@opencode-ai/sdk";

const RETRY_PROMPT = "continue";
const QUICK_FAILURE_WINDOW_MS = 15_000;
const MESSAGE_POLL_ATTEMPTS = 5;
const MESSAGE_POLL_DELAY_MS = 200;
const MESSAGE_FETCH_LIMIT = 24;

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

const states =
	globalState.__opencodeRetryServerErrors ??
	(globalState.__opencodeRetryServerErrors = new Map<string, RetryState>());

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const getState = (sessionID: string): RetryState => {
	const existing = states.get(sessionID);
	if (existing) return existing;

	const created: RetryState = {
		backoffStep: 0,
		lastFailureAt: 0,
	};
	states.set(sessionID, created);
	return created;
};

const safeJsonParse = (value: string): unknown => {
	try {
		return JSON.parse(value);
	} catch {
		return undefined;
	}
};

const isObject = (value: unknown): value is Record<string, unknown> =>
	typeof value === "object" && value !== null;

const unwrapProviderError = (message: string) => {
	const parsed = safeJsonParse(message);
	if (!isObject(parsed)) return undefined;
	if (parsed.type !== "error") return undefined;

	const inner = parsed.error;
	return isObject(inner) ? inner : undefined;
};

const isRetryableError = (error: AssistantMessage["error"] | undefined): boolean => {
	if (!error) return false;

	const rawMessage =
		error && "data" in error && isObject(error.data) && typeof error.data.message === "string"
			? error.data.message
			: "";
	const nested = rawMessage ? unwrapProviderError(rawMessage) : undefined;
	const nestedCode = typeof nested?.code === "string" ? nested.code : "";
	const nestedType = typeof nested?.type === "string" ? nested.type : "";
	const nestedMessage = typeof nested?.message === "string" ? nested.message : "";

	return (
		nestedCode === "server_error" ||
		nestedCode === "server_is_overloaded" ||
		nestedType === "server_error" ||
		nestedType === "service_unavailable_error" ||
		rawMessage.includes("An error occurred while processing your request") ||
		rawMessage.includes("Our servers are currently overloaded") ||
		nestedMessage.includes("An error occurred while processing your request") ||
		nestedMessage.includes("Our servers are currently overloaded")
	);
};

const sortNewestFirst = <T extends Message>(messages: T[]): T[] =>
	[...messages].sort((a, b) => b.time.created - a.time.created);

const fetchMessages = async (client: Parameters<Plugin>[0]["client"], sessionID: string) => {
	const response = await client.session.messages({
		path: { id: sessionID },
		query: { limit: MESSAGE_FETCH_LIMIT },
	});
	return response.data ?? [];
};

const pollRetryableTail = async (
	client: Parameters<Plugin>[0]["client"],
	sessionID: string,
): Promise<AssistantMessage | undefined> => {
	for (let attempt = 0; attempt < MESSAGE_POLL_ATTEMPTS; attempt++) {
		const entries = await fetchMessages(client, sessionID);
		const infos = entries.map((entry) => entry.info);
		const tail = sortNewestFirst(infos)[0];

		if (
			tail &&
			tail.role === "assistant" &&
			isRetryableError(tail.error)
		) {
			return tail;
		}

		if (attempt + 1 < MESSAGE_POLL_ATTEMPTS) {
			await wait(MESSAGE_POLL_DELAY_MS);
		}
	}

	return undefined;
};

const hasNewerMessages = async (
	client: Parameters<Plugin>[0]["client"],
	sessionID: string,
	since: number,
): Promise<boolean> => {
	const entries = await fetchMessages(client, sessionID);
	return entries.some((entry) => entry.info.time.created > since);
};

const fetchMessageEntry = async (
	client: Parameters<Plugin>[0]["client"],
	sessionID: string,
	messageID: string,
): Promise<{ info: Message; parts: Array<{ type: string; text?: string; synthetic?: boolean; ignored?: boolean }> } | undefined> => {
	const response = await client.session.message({
		path: {
			id: sessionID,
			messageID,
		},
	});
	return response.data as
		| { info: Message; parts: Array<{ type: string; text?: string; synthetic?: boolean; ignored?: boolean }> }
		| undefined;
};

const deleteMessage = async (client: Parameters<Plugin>[0]["client"], sessionID: string, messageID: string): Promise<void> => {
	const transport = (client.session as any)?._client ?? (client as any)?._client;
	if (!transport?.delete) {
		throw new Error("OpenCode SDK transport does not expose delete()");
	}

	await transport.delete({
		url: "/session/{id}/message/{messageID}",
		path: {
			id: sessionID,
			messageID,
		},
	});
};

const isPlainContinueMessage = (
	entry: { info: Message; parts: Array<{ type: string; text?: string; synthetic?: boolean; ignored?: boolean }> } | undefined,
): entry is { info: UserMessage; parts: Array<{ type: string; text?: string; synthetic?: boolean; ignored?: boolean }> } => {
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

const getAssistantChildren = (
	entries: Array<{ info: Message; parts: Array<{ type: string; text?: string; synthetic?: boolean; ignored?: boolean }> }>,
	parentID: string,
): AssistantMessage[] =>
	entries
		.map((entry) => entry.info)
		.filter(
			(message): message is AssistantMessage =>
				message.role === "assistant" && message.parentID === parentID,
		);

const removeRetryChainForParent = async (
	client: Parameters<Plugin>[0]["client"],
	sessionID: string,
	parentID: string,
	entries: Array<{ info: Message; parts: Array<{ type: string; text?: string; synthetic?: boolean; ignored?: boolean }> }>,
	skipMessageID: string | undefined,
	log: (level: "debug" | "info" | "warn" | "error", message: string, extra?: Record<string, unknown>) => Promise<void>,
) => {
	const children = getAssistantChildren(entries, parentID).filter((child) => child.id !== skipMessageID);
	const hasSuccessfulChild = children.some((child) => !child.error);
	if (hasSuccessfulChild) {
		return false;
	}

	for (const child of children) {
		await deleteMessage(client, sessionID, child.id).catch(async (deleteError: unknown) => {
			const message = deleteError instanceof Error ? deleteError.message : String(deleteError);
			await log("warn", "failed to delete sibling retry message", {
				sessionID,
				messageID: child.id,
				parentID,
				error: message,
			});
		});
	}

	await deleteMessage(client, sessionID, parentID).catch(async (deleteError: unknown) => {
		const message = deleteError instanceof Error ? deleteError.message : String(deleteError);
		await log("warn", "failed to delete retry parent continue message", {
			sessionID,
			messageID: parentID,
			error: message,
		});
	});

	return true;
};

const scheduleDelayMs = (state: RetryState, now: number): number => {
	if (state.lastFailureAt && now - state.lastFailureAt < QUICK_FAILURE_WINDOW_MS) {
		state.backoffStep += 1;
	} else {
		state.backoffStep = 0;
	}

	state.lastFailureAt = now;
	return state.backoffStep * 1000;
};

export const RetryServerErrorsPlugin: Plugin = async ({ client }) => {
	const log = async (level: "debug" | "info" | "warn" | "error", message: string, extra?: Record<string, unknown>) => {
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

	const toast = async (message: string, variant: "info" | "warning" | "error" = "warning") => {
		await client.tui
			.showToast({
				body: {
					message,
					variant,
					duration: 2500,
				},
			})
			.catch(() => undefined);
	};

	return {
		event: async ({ event }) => {
			if (event.type !== "session.error") return;

			const { sessionID, error } = event.properties;
			if (!sessionID || !isRetryableError(error)) return;

			const failingMessage = await pollRetryableTail(client, sessionID);
			if (!failingMessage) return;

			const state = getState(sessionID);
			if (state.lastHandledMessageID === failingMessage.id) return;
			state.lastHandledMessageID = failingMessage.id;

			if (state.pendingTimer) {
				clearTimeout(state.pendingTimer);
				state.pendingTimer = undefined;
			}

			const delayMs = scheduleDelayMs(state, Date.now());
			const token = Symbol(failingMessage.id);
			state.pendingToken = token;

			const recentEntries = await fetchMessages(client, sessionID);

			await deleteMessage(client, sessionID, failingMessage.id).catch(async (deleteError: unknown) => {
				const message = deleteError instanceof Error ? deleteError.message : String(deleteError);
				await log("warn", "failed to delete retryable error message", {
					sessionID,
					messageID: failingMessage.id,
					error: message,
				});
			});

			const parentEntry = await fetchMessageEntry(client, sessionID, failingMessage.parentID).catch(
				() => undefined,
			);
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

			if (delayMs > 0) {
				await toast(`Retrying in ${delayMs / 1000}s`, "warning");
			}

			state.pendingTimer = setTimeout(async () => {
				if (state.pendingToken !== token) return;

				try {
					if (await hasNewerMessages(client, sessionID, failingMessage.time.created)) {
						await log("info", "skipped retry because the session moved forward", {
							sessionID,
							messageID: failingMessage.id,
						});
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

					await log("info", "scheduled retry for transient provider error", {
						sessionID,
						messageID: failingMessage.id,
						delayMs,
						agent: parentUserMessage?.agent,
						modelID: failingMessage.modelID,
						providerID: failingMessage.providerID,
					});
				} catch (retryError) {
					const message = retryError instanceof Error ? retryError.message : String(retryError);
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
