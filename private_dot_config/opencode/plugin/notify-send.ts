import path from "path";
const processedSessions = new Set();
export const NotificationPlugin = async ({
	project,
	client,
	$,
	directory,
	worktree,
}) => {
	return {
		event: async ({ event }) => {
			if (event.type === "session.idle") {
				const sessionId = event.properties.sessionID;

				// Check and add atomically
				if (processedSessions.has(sessionId)) {
					return;
				}
				processedSessions.add(sessionId);

				// Add cleanup delay to handle race conditions
				setTimeout(() => {
					processedSessions.delete(sessionId);
				}, 100);

				const session = await client.session.get({
					path: { id: sessionId },
				});
				const title = session.data?.title || "Session";
				await $`notify-send "OpenCode" "${title} completed!" \
          --urgency=normal \
          --icon=starred \
          --category=opencode \
          --app-name=OpenCode \
          --expire-time=10000`;
			}
		},
	};
};
