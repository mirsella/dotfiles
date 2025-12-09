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
				title = session.data?.title || "Session";
			} catch {}

			const message = `OpenCode ${title} completed!`;

			let phoneConnected = false;
			try {
				const kdeconnectResult = await $`kdeconnect-cli --list-devices`
					.quiet()
					.nothrow();
				const stdout = String(kdeconnectResult.stdout || kdeconnectResult.text || kdeconnectResult);
				phoneConnected = stdout
					.split("\n")
					.some((line) => line.includes("phone") && line.includes("reachable"));
			} catch {}

			let shouldFallback = false;
			let errorMessage = "";

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

			if (!shouldFallback) {
				return;
			}

			if (errorMessage) {
				await $`notify-send "OpenCode Error" "${errorMessage}" --urgency=critical --icon=dialog-error --category=opencode.error --app-name=OpenCode --expire-time=10000`.nothrow();
			}

			await $`notify-send "OpenCode" "${message}" --urgency=normal --icon=starred --category=opencode --app-name=OpenCode --expire-time=6000`.nothrow();
		},
	};
};
