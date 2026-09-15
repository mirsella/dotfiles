import { expect, test } from "bun:test";
import type { Hooks, PluginInput } from "@opencode-ai/plugin";
import { DoomLoopThresholdPlugin } from "./doom-loop-threshold";

test("rejects the sixth identical tool call", async () => {
	const responses: string[] = [];
	const client = {
		postSessionIdPermissionsPermissionId: async ({ body }: { body: { response: string } }) => {
			responses.push(body.response);
		},
	};
	const hooks = await DoomLoopThresholdPlugin({ client } as unknown as PluginInput, { threshold: 6 });
	const event = hooks.event as NonNullable<Hooks["event"]>;

	for (let index = 0; index < 4; index++) {
		await event({
			event: {
				type: "permission.asked",
				properties: {
					id: String(index),
					sessionID: "session",
					permission: "doom_loop",
					metadata: { tool: "read", input: { filePath: "file" } },
				},
			} as never,
		});
	}

	expect(responses).toEqual(["once", "once", "once", "reject"]);
});
