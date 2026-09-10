import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
	DEFAULT_CONFIG,
	RecoveryStore,
	SubagentWatchdog,
	normalizeConfig,
	type RecoveryCounter,
	type SessionInfo,
	type SessionStatus,
	type WatchdogApi,
} from "../lib/subagent-watchdog";

class MemoryStore implements RecoveryCounter {
	counts = new Map<string, number>();
	onReserve?: () => void;
	count(child: string) {
		return this.counts.get(child) ?? 0;
	}
	async reserve(child: string, limit: number) {
		const previous = this.count(child);
		const attempt = previous + 1;
		if (attempt > limit) return;
		this.counts.set(child, attempt);
		this.onReserve?.();
		return {
			attempt,
			rollback: async () => {
				if (this.count(child) !== attempt) return;
				if (previous) this.counts.set(child, previous);
				else this.counts.delete(child);
			},
		};
	}
	async cleanup() {}
}

class FakeApi implements WatchdogApi {
	sessions = new Map<string, SessionInfo>();
	statuses: Record<string, SessionStatus> = {};
	pending = new Set<string>();
	taskState: {
		callID: string;
		status: "running" | "completed" | "error";
		latest: boolean;
	} = { callID: "call_original", status: "running", latest: true };
	aborts: string[] = [];
	prompts: Array<{ sessionID: string; prompt: string }> = [];
	onSnapshot?: () => void;
	onAbort?: () => void | Promise<void>;

	async snapshot() {
		this.onSnapshot?.();
		return {
			sessions: [...this.sessions.values()],
			statuses: { ...this.statuses },
			pending: new Set(this.pending),
		};
	}
	async getStatuses() {
		return { ...this.statuses };
	}
	async getTaskState(_parentSessionID: string, childSessionID: string) {
		return childSessionID === child.id ? { ...this.taskState } : undefined;
	}
	async abort(sessionID: string) {
		this.aborts.push(sessionID);
		this.statuses[sessionID] = { type: "idle" };
		for (const session of this.sessions.values())
			if (session.parentID === sessionID) this.statuses[session.id] = { type: "idle" };
		this.taskState = { ...this.taskState, status: "error" };
		await this.onAbort?.();
	}
	async promptAsync(sessionID: string, prompt: string) {
		this.prompts.push({ sessionID, prompt });
	}
}

const parent: SessionInfo = { id: "ses_parent", title: "Parent task", time: { updated: 0 } };
const child: SessionInfo = {
	id: "ses_child",
	title: "Investigate networking",
	parentID: parent.id,
	time: { updated: 0 },
};

function setup(initialRecoveries = 0, overrides: Partial<typeof DEFAULT_CONFIG> = {}) {
	let now = 0;
	const api = new FakeApi();
	api.sessions.set(parent.id, parent);
	api.sessions.set(child.id, child);
	api.statuses[parent.id] = { type: "busy" };
	api.statuses[child.id] = { type: "busy" };
	const store = new MemoryStore();
	if (initialRecoveries) store.counts.set(child.id, initialRecoveries);
	const logs: string[] = [];
	const watchdog = new SubagentWatchdog(
		{ ...DEFAULT_CONFIG, ...overrides },
		api,
		store,
		async (_level, event) => {
			logs.push(event);
		},
		async () => {},
		() => now,
		async (ms) => {
			now += ms;
		},
	);
	const setNow = (value: number) => {
		now = value;
	};
	return { api, watchdog, store, logs, setNow };
}

async function tickAt(watchdog: SubagentWatchdog, setNow: (value: number) => void, now: number) {
	setNow(now);
	await watchdog.tick();
}

function addSibling(api: FakeApi) {
	const sibling = { ...child, id: "ses_sibling" };
	api.sessions.set(sibling.id, sibling);
	api.statuses[sibling.id] = { type: "busy" };
}

async function taskPart(
	watchdog: SubagentWatchdog,
	callID: string,
	start = 181_000,
) {
	await watchdog.handleEvent({
		type: "message.part.updated",
		properties: {
			part: {
				type: "tool",
				tool: "task",
				sessionID: parent.id,
				callID,
				state: { status: "running", time: { start }, metadata: { sessionId: child.id } },
			},
		},
	});
}

async function taskBefore(
	watchdog: SubagentWatchdog,
	callID: string,
	args: Record<string, unknown>,
) {
	await watchdog.handleToolBefore({ tool: "task", sessionID: parent.id, callID }, { args });
}

describe("subagent watchdog", () => {
	test("normal child activity never becomes suspect", async () => {
		const { api, watchdog, logs, setNow } = setup();
		await watchdog.tick();
		for (let seconds = 20; seconds <= 240; seconds += 20) {
			setNow(seconds * 1_000);
			watchdog.recordActivity(child.id);
			await watchdog.tick();
		}
		expect(api.aborts).toEqual([]);
		expect(logs).not.toContain("watchdog.child.suspect");
	});

	test("temporary silence resets after activity", async () => {
		const { api, watchdog, logs, setNow } = setup();
		await watchdog.tick();
		await tickAt(watchdog, setNow, 90_000);
		expect(logs).toContain("watchdog.child.suspect");
		watchdog.recordActivity(child.id);
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
	});

	test("stalled child aborts once and resumes the exact task ID", async () => {
		const { api, watchdog, logs, setNow } = setup();
		await watchdog.tick();
		await taskBefore(watchdog, "call_original", {
			description: "Investigate networking",
			prompt: "Inspect the networking implementation",
			subagent_type: "explore",
			task_id: child.id,
		});
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([parent.id]);
		expect(api.prompts).toHaveLength(1);
		expect(api.prompts[0].prompt).toContain(child.id);
		expect(api.prompts[0].prompt).toContain("explore");
		await taskPart(watchdog, "call_original", 0);
		expect(logs).not.toContain("watchdog.child.recovered");

		const output = {
			args: { description: "Resume child", prompt: "Continue", subagent_type: "general" },
		};
		await taskBefore(watchdog, "call_resume", output.args);
		expect(output.args).toMatchObject({
			task_id: child.id,
			subagent_type: "explore",
			background: false,
		});
		await taskPart(watchdog, "call_resume");
		expect(logs).toContain("watchdog.child.recovered");
	});

	test("progress during revalidation cancels the abort", async () => {
		const { api, watchdog, logs, setNow } = setup();
		await watchdog.tick();
		let snapshots = 0;
		api.onSnapshot = () => {
			if (++snapshots === 2) watchdog.recordActivity(child.id);
		};
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
		expect(logs).toContain("watchdog.child.recovery_cancelled");
	});

	test("progress while reserving does not consume recovery", async () => {
		const { api, watchdog, store, setNow } = setup();
		await watchdog.tick();
		store.onReserve = () => watchdog.recordActivity(child.id);
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
		expect(store.count(child.id)).toBe(0);
	});

	test("snapshot failure while reserving does not consume recovery", async () => {
		const { api, watchdog, store, setNow } = setup();
		await watchdog.tick();
		let snapshots = 0;
		api.onSnapshot = () => {
			if (++snapshots === 3) throw new Error("temporary API failure");
		};
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
		expect(store.count(child.id)).toBe(0);
	});

	test("failed abort does not consume recovery", async () => {
		const { api, watchdog, store, setNow } = setup();
		await watchdog.tick();
		api.onAbort = () => {
			throw new Error("abort failed");
		};
		await tickAt(watchdog, setNow, 181_000);
		expect(store.count(child.id)).toBe(0);
	});

	test("completed or superseded work is not prompted after abort", async () => {
		for (const mutate of [
			(api: FakeApi) => (api.taskState.status = "completed"),
			(api: FakeApi) => (api.taskState.latest = false),
		]) {
			const { api, watchdog, setNow } = setup();
			await watchdog.tick();
			api.onAbort = () => void mutate(api);
			await tickAt(watchdog, setNow, 181_000);
			expect(api.aborts).toEqual([parent.id]);
			expect(api.prompts).toEqual([]);
		}
	});

	test("recovery prompts even when the parent never becomes idle", async () => {
		const { api, watchdog, logs, setNow } = setup();
		await watchdog.tick();
		api.onAbort = () => {
			api.statuses[parent.id] = { type: "busy" };
		};
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([parent.id]);
		expect(logs).toContain("watchdog.child.parent_not_idle");
		expect(api.prompts).toHaveLength(1);
		expect(api.prompts[0].prompt).toContain(child.id);
	});

	test("pending permission or question disables recovery", async () => {
		const { api, watchdog, setNow } = setup();
		api.pending.add(parent.id);
		await watchdog.tick();
		await tickAt(watchdog, setNow, 10 * 60_000);
		expect(api.aborts).toEqual([]);
	});

	test("scheduled provider retry is not treated as a stall", async () => {
		const { api, watchdog, setNow } = setup();
		api.statuses[child.id] = {
			type: "retry", attempt: 1, message: "provider retry", next: 5 * 60_000,
		};
		await watchdog.tick();
		await tickAt(watchdog, setNow, 4 * 60_000);
		expect(api.aborts).toEqual([]);
	});

	test("active tool uses the longer timeout", async () => {
		const { api, watchdog, setNow } = setup();
		await watchdog.tick();
		await watchdog.handleToolBefore(
			{ tool: "bash", sessionID: child.id, callID: "call_tool" },
			{ args: {} },
		);
		await tickAt(watchdog, setNow, 5 * 60_000);
		expect(api.aborts).toEqual([]);
	});

	test("parallel children skip parent recovery", async () => {
		const { api, watchdog, logs, setNow } = setup();
		addSibling(api);
		await watchdog.tick();
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
		expect(logs).toContain("watchdog.child.parallel_skip");
	});

	test("opt-in parallel recovery starts only once per parent", async () => {
		const { api, watchdog, setNow } = setup(0, { recoverParallelChildren: true });
		addSibling(api);
		await watchdog.tick();
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([parent.id]);
		expect(api.prompts).toHaveLength(1);
	});

	test("background metadata survives arriving before child creation", async () => {
		const { api, watchdog, setNow } = setup();
		await watchdog.tick();
		api.sessions.delete(child.id);
		delete api.statuses[child.id];
		await watchdog.tick();
		await taskBefore(watchdog, "call_background", {
			description: "Investigate networking",
			prompt: "Inspect the networking implementation",
			subagent_type: "explore",
			background: true,
		});
		await watchdog.handleToolAfter(
			{ tool: "task", sessionID: parent.id, callID: "call_background" },
			{ metadata: { sessionId: child.id } },
		);
		api.sessions.set(child.id, child);
		api.statuses[child.id] = { type: "busy" };
		await watchdog.handleEvent({ type: "session.created", properties: { info: child } });
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
		api.statuses[parent.id] = { type: "idle" };
		await watchdog.tick();
		expect(api.aborts).toEqual([]);
	});

	test("expired resume expectation cannot rewrite a later task", async () => {
		const { watchdog, logs, setNow } = setup();
		await watchdog.tick();
		await tickAt(watchdog, setNow, 181_000);
		await tickAt(watchdog, setNow, 362_000);
		expect(logs).toContain("watchdog.child.recovery_failed");
		const output = {
			args: { description: "Different task", prompt: "Start new work", subagent_type: "general" },
		};
		await watchdog.handleToolBefore(
			{ tool: "task", sessionID: parent.id, callID: "call_later" },
			output,
		);
		expect(output.args).not.toHaveProperty("task_id");
	});

	test("task-part metadata confirms recovery when the before hook was missed", async () => {
		const { watchdog, setNow, logs } = setup();
		await watchdog.tick();
		await tickAt(watchdog, setNow, 181_000);
		await taskPart(watchdog, "call_event_only");
		expect(logs).toContain("watchdog.child.recovered");
	});

	test("a second stall is reported but not recovered", async () => {
		const { api, watchdog, logs, setNow } = setup(1);
		await watchdog.tick();
		await tickAt(watchdog, setNow, 181_000);
		expect(api.aborts).toEqual([]);
		expect(logs).toContain("watchdog.child.recovery_limit");
	});
});

describe("recovery persistence", () => {
	const paths: string[] = [];
	afterEach(async () => {
		await Promise.all(paths.splice(0).map((path) => rm(path, { recursive: true })));
	});

	test("a restart preserves the recovery limit", async () => {
		const directory = await mkdtemp(join(tmpdir(), "opencode-watchdog-"));
		paths.push(directory);
		const path = join(directory, "state.json");
		const first = await RecoveryStore.open(path, 1_000);
		await first.reserve(child.id, 1, 1_000, {
			childTitle: child.title,
			parentSessionID: parent.id,
			parentTitle: parent.title,
		});
		expect(JSON.parse(await readFile(path, "utf8"))).toEqual({
			totalRecoveries: 1,
			sessions: {
				[child.id]: {
					recoveryCount: 1,
					lastRecoveryAt: 1_000,
					childTitle: child.title,
					parentSessionID: parent.id,
					parentTitle: parent.title,
				},
			},
		});
		const restarted = await RecoveryStore.open(path, 2_000);
		expect(restarted.count(child.id)).toBe(1);
	});

	test("malformed configuration values fall back safely", () => {
		const { config, warnings } = normalizeConfig({
			mode: "unsafe",
			recoverAfterMs: -1,
			maxRecoveriesPerChild: 1.5,
		});
		expect(config).toEqual(DEFAULT_CONFIG);
		expect(warnings).toHaveLength(3);
	});

	test("malformed recovery state fails closed", async () => {
		const directory = await mkdtemp(join(tmpdir(), "opencode-watchdog-"));
		paths.push(directory);
		const path = join(directory, "state.json");
		await writeFile(path, "not json");
		await expect(RecoveryStore.open(path)).rejects.toBeDefined();
	});
});
