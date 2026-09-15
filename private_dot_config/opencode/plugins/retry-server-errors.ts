import type { Plugin } from "@opencode-ai/plugin";
import type {
	AssistantMessage,
	Message,
	Part,
} from "@opencode-ai/sdk";
import { createOpencodeClient } from "@opencode-ai/sdk/v2";

const RETRY_PROMPT = "continue";
const MESSAGE_FETCH_LIMIT = 24;

type Client = Parameters<Plugin>[0]["client"];
type MessageEntry = { info: Message; parts: Part[] };

const errorMessage = (error: unknown) =>
	error instanceof Error ? error.message : String(error);

const messages = async (client: Client, sessionID: string) =>
	(
		await client.session.messages({
			path: { id: sessionID },
			query: { limit: MESSAGE_FETCH_LIMIT },
			throwOnError: true,
		})
	).data;

const latestMessage = (entries: MessageEntry[]) =>
	entries.reduce<MessageEntry | undefined>(
		(latest, entry) =>
			!latest || entry.info.time.created > latest.info.time.created
				? entry
				: latest,
		undefined,
	);

const isRetryPrompt = (entry: MessageEntry) => {
	const part = entry.parts[0];
	return (
		entry.info.role === "user" &&
		entry.parts.length === 1 &&
		part?.type === "text" &&
		!part.synthetic &&
		!part.ignored &&
		part.text.trim() === RETRY_PROMPT
	);
};

export const RetryServerErrorsPlugin: Plugin = async ({
	client,
	directory,
	serverUrl,
}) => {
	const api = createOpencodeClient({
		baseUrl: serverUrl.toString(),
		directory,
	});
	const handled = new Set<string>();

	const log = async (
		level: "info" | "warn" | "error",
		message: string,
		extra?: Record<string, unknown>,
	) => {
		await client.app
			.log({
				body: { service: "retry-server-errors", level, message, extra },
			})
			.catch(() => undefined);
	};

	const retry = async (failed: AssistantMessage) => {
		if (handled.has(failed.id)) return;
		handled.add(failed.id);

		try {
			const entries = await messages(client, failed.sessionID);
			if (latestMessage(entries)?.info.id !== failed.id) {
				await log("info", "skipped stale provider retry", {
					sessionID: failed.sessionID,
					messageID: failed.id,
				});
				return;
			}

			const parentEntry =
				entries.find((entry) => entry.info.id === failed.parentID) ??
				(
					await client.session.message({
						path: {
							id: failed.sessionID,
							messageID: failed.parentID,
						},
						throwOnError: true,
					})
				).data;
			if (parentEntry.info.role !== "user") {
				throw new Error(`Assistant message ${failed.id} has no user parent`);
			}
			const parent = parentEntry.info;

			const remove = async (messageID: string) => {
				try {
					await api.session.deleteMessage(
						{ sessionID: failed.sessionID, messageID },
						{ throwOnError: true },
					);
					return true;
				} catch (error) {
					await log("warn", "failed to delete retry message", {
						sessionID: failed.sessionID,
						messageID,
						error: errorMessage(error),
					});
					return false;
				}
			};

			let chainDeleted = await remove(failed.id);
			if (isRetryPrompt(parentEntry)) {
				const siblings: AssistantMessage[] = [];
				for (const entry of entries) {
					if (
						entry.info.role === "assistant" &&
						entry.info.parentID === parent.id &&
						entry.info.id !== failed.id
					) {
						siblings.push(entry.info);
					}
				}

				if (siblings.every((message) => message.error)) {
					for (const sibling of siblings) {
						const deleted = await remove(sibling.id);
						chainDeleted = deleted && chainDeleted;
					}
					if (chainDeleted) await remove(parent.id);
				}
			}

			const current = latestMessage(
				await messages(client, failed.sessionID),
			);
			if (current && current.info.time.created > failed.time.created) {
				await log("info", "skipped retry because the session moved forward", {
					sessionID: failed.sessionID,
					messageID: failed.id,
				});
				return;
			}

			await client.session.promptAsync({
				path: { id: failed.sessionID },
				body: {
					agent: parent.agent,
					model: parent.model,
					parts: [{ type: "text", text: RETRY_PROMPT }],
				},
				throwOnError: true,
			});
			await log("info", "retried transient provider error", {
				sessionID: failed.sessionID,
				messageID: failed.id,
				modelID: failed.modelID,
				providerID: failed.providerID,
			});
		} catch (error) {
			const message = errorMessage(error);
			await log("error", "failed to retry provider error", {
				sessionID: failed.sessionID,
				messageID: failed.id,
				error: message,
			});
			await client.tui
				.showToast({
					body: {
						message: `Provider retry failed: ${message}`,
						variant: "error",
						duration: 2500,
					},
				})
				.catch(() => undefined);
		}
	};

	return {
		event: async ({ event }) => {
			if (event.type !== "message.updated") return;
			const message = event.properties.info;
			if (
				message.role === "assistant" &&
				message.error?.name === "APIError" &&
				message.error.data.isRetryable
			) {
				await retry(message);
			}
		},
	};
};
