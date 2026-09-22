import type { Plugin } from "@opencode-ai/plugin";
import type { AssistantMessage } from "@opencode-ai/sdk";
import timers from "node:timers/promises";

const RETRY_DELAY_MS = 10_000;

const isDatabaseLockedError = (error: AssistantMessage["error"]) =>
	error !== undefined && [error.name, ...Object.values(error.data)].some(
		value => typeof value === "string" && /database(?: is)? locked|sqlite_(?:busy|locked)/i.test(value),
	);

export const RetryDatabaseLockedPlugin: Plugin = async ({ client }) => {
	const pending = new Map<string, AbortController>();
	const log = (level: "info" | "warn" | "error", message: string, extra: Record<string, unknown>) =>
		client.app.log({ body: { service: "retry-database-locked", level, message, extra }, throwOnError: true });

	const latest = async (sessionID: string) => {
		const { data } = await client.session.messages({
			path: { id: sessionID }, query: { limit: 1 }, throwOnError: true,
		});
		const message = data[0]?.info;
		if (!message) throw new Error(`Cannot retry session ${sessionID}: no message to resume`);
		return message;
	};

	const retry = async (sessionID: string, controller: AbortController) => {
		const { signal } = controller;
		try {
			const entry = await latest(sessionID);
			const parent = entry.role === "user" ? entry : (
				await client.session.message({
					path: { id: sessionID, messageID: entry.parentID }, throwOnError: true,
				})
			).data.info;
			if (parent.role !== "user") throw new Error(`Message ${entry.id} has no user parent`);
			if (signal.aborted) return;
			await log("warn", "database locked; retrying in 10s", { sessionID, messageID: entry.id });
			await client.tui.showToast({
				body: { message: "Database locked; retrying in 10s", variant: "warning", duration: 2500 },
				throwOnError: true,
			});

			while (!signal.aborted) {
				await timers.setTimeout(RETRY_DELAY_MS, undefined, { signal });
				try {
					if ((await latest(sessionID)).id !== entry.id) {
						await log("info", "skipped retry because the session moved forward", { sessionID });
						return;
					}
					if (signal.aborted) return;
					await client.session.promptAsync({
						path: { id: sessionID },
						body: {
							agent: parent.agent,
							model: parent.model,
							parts: [{ type: "text", text: "continue" }],
						},
						throwOnError: true,
					});
				} catch (error) {
					if (signal.aborted) return;
					await log("error", "retry failed; retrying in 10s", {
						sessionID, error: error instanceof Error ? error.message : error,
					});
					continue;
				}
				await log("info", "resumed after database lock", { sessionID, messageID: entry.id });
				return;
			}
		} catch (error) {
			if (!signal.aborted) {
				await log("error", "database lock recovery failed", {
					sessionID, error: error instanceof Error ? error.message : error,
				});
			}
		} finally {
			if (pending.get(sessionID) === controller) pending.delete(sessionID);
		}
	};

	return {
		event: async ({ event }) => {
			if (event.type === "session.deleted") {
				pending.get(event.properties.info.id)?.abort();
				return;
			}
			if (event.type !== "session.error") return;
			const { sessionID, error } = event.properties;
			if (!sessionID || !isDatabaseLockedError(error)) return;
			pending.get(sessionID)?.abort();
			const controller = new AbortController();
			pending.set(sessionID, controller);
			void retry(sessionID, controller).catch(error => {
				// Recovery uses the API too; its logging endpoint can also be unavailable.
				console.error("database lock recovery could not log its failure", error);
			});
		},
	};
};
