import type { Plugin } from "@opencode-ai/plugin";

export const NotificationPlugin: Plugin = async ({
	project,
	client,
	$,
	directory,
	worktree,
}) => {
	return {
		event: async ({ event }) => {
			// Send notification on session completion
			if (event.type === "session.idle") {
				// NOTE: temporarily disabled becuase of a bug, the event is fired 6 times and error: GDBus.Error:org.freedesktop.Notifications.Error.ExcessNotificationGeneration: Created too many similar notifications in quick succession
				// await $`notify-send -a "opencode" "Session completed!"`;
			}
		},
	};
};
