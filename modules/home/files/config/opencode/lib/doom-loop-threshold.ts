import type { Plugin } from "@opencode-ai/plugin";
import type { Event } from "@opencode-ai/sdk/v2";

type State = { key: string; prompts: number };

export const DoomLoopThresholdPlugin: Plugin = async ({ client }, options) => {
	const threshold = options?.threshold;
	if (typeof threshold !== "number" || !Number.isInteger(threshold) || threshold < 3) {
		throw new Error("doom-loop-threshold requires an integer threshold of at least 3");
	}

	const states = new Map<string, State>();
	const stateFor = (sessionID: string, tool: string, input: unknown) => {
		const key = JSON.stringify([tool, input]);
		const state = states.get(sessionID);
		if (state?.key === key) return state;
		const created = { key, prompts: 0 };
		states.set(sessionID, created);
		return created;
	};

	return {
		"tool.execute.before": async (input, output) => {
			stateFor(input.sessionID, input.tool, output.args);
		},
		event: async ({ event: legacyEvent }) => {
			// The plugin package still types runtime v2 events as the legacy event union.
			const event = legacyEvent as Event;
			if (event.type === "session.idle") {
				states.delete(event.properties.sessionID);
				return;
			}
			if (event.type !== "permission.asked") return;

			const request = event.properties;
			const tool = request.metadata.tool;
			if (request.permission !== "doom_loop" || typeof tool !== "string") return;

			const state = stateFor(request.sessionID, tool, request.metadata.input);
			const response = ++state.prompts >= threshold - 2 ? "reject" : "once";
			if (response === "reject") states.delete(request.sessionID);
			await client.postSessionIdPermissionsPermissionId({
				path: { id: request.sessionID, permissionID: request.id },
				body: { response },
			});
		},
	};
};
