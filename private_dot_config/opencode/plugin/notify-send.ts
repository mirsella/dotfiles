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
				const message = `OpenCode ${title} completed!`;

				// Check if "phone" device is connected via kdeconnect
				const kdeconnectResult = await $`kdeconnect-cli --list-devices`.quiet();
				const phoneConnected = kdeconnectResult.stdout
					.split("\n")
					.some((line) => line.includes("phone") && line.includes("reachable"));

				let shouldFallback = false;
				let errorMessage = "";

				// Send notification to phone via Telegram
				const tgId = process.env.TgId;
				const tgToken = process.env.TgToken;

				if (!tgId || !tgToken) {
					shouldFallback = true;
					errorMessage = "Telegram env vars (TgId/TgToken) not set";
				} else {
					try {
						const response = await fetch(
							`https://api.telegram.org/bot${tgToken}/sendMessage`,
							{
								method: "POST",
								headers: { "Content-Type": "application/json" },
								body: JSON.stringify({
									chat_id: tgId,
									text: message,
								}),
								signal: AbortSignal.timeout(2000),
							},
						);
						if (!response.ok) {
							shouldFallback = true;
							errorMessage = `Telegram API error: ${response.status}`;
						}
					} catch (error) {
						shouldFallback = true;
						errorMessage = `Telegram fetch failed: ${error instanceof Error ? error.message : "Unknown error"}`;
					}
				}

				if (!phoneConnected) {
					shouldFallback = true;
				}

				if (shouldFallback) {
					// Send error notification if there was an issue with Telegram
					if (errorMessage) {
						await $`notify-send "OpenCode Error" "${errorMessage}" \
          --urgency=critical \
          --icon=dialog-error \
          --category=opencode.error \
          --app-name=OpenCode \
          --expire-time=15000`;
					}

					// Send the main notification as fallback
					await $`notify-send "OpenCode" "${message}" \
          --urgency=normal \
          --icon=starred \
          --category=opencode \
          --app-name=OpenCode \
          --expire-time=10000`;
				}
			}
		},
	};
};
