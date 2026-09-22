import { afterEach, beforeEach, expect, mock, spyOn, test } from "bun:test";
import type { PluginInput } from "@opencode-ai/plugin";
import timers from "node:timers/promises";
import { RetryDatabaseLockedPlugin } from "../plugins/retry-database-locked";

const waits: Array<{ signal: AbortSignal; resolve: () => void }> = [];
const cleanup: Array<() => Promise<void>> = [];
const flush = () => Bun.sleep(0);
let clock: ReturnType<typeof spyOn>;

beforeEach(() => {
	waits.length = 0;
	clock = spyOn(timers, "setTimeout").mockImplementation((ms, _value, options) => {
		expect(ms).toBe(10_000);
		const signal = options!.signal!;
		return new Promise((resolve, reject) => {
			const abort = () => reject(signal.reason);
			const wake = () => { signal.removeEventListener("abort", abort); resolve(undefined); };
			signal.addEventListener("abort", abort, { once: true });
			waits.push({ signal, resolve: wake });
		});
	});
});

afterEach(async () => {
	await Promise.all(cleanup.splice(0).map(fn => fn()));
	await flush();
	clock.mockRestore();
});

async function setup() {
	type Client = PluginInput["client"];
	const parent = {
		id: "user", role: "user" as const, agent: "plan",
		time: { created: 1000 },
		model: { providerID: "llama.cpp", modelID: "local", variant: "thinking" },
	};
	const failed = { id: "failed", role: "assistant" as const, parentID: parent.id, time: { created: 1001 } };
	const session = {
		messages: mock(async (_: Parameters<Client["session"]["messages"]>[0]): Promise<{
			data: Array<{ info: typeof failed | typeof parent }>;
		}> => ({ data: [{ info: failed }] })),
		message: mock(async (_: Parameters<Client["session"]["message"]>[0]) => ({ data: { info: parent } })),
		promptAsync: mock(async (_: Parameters<Client["session"]["promptAsync"]>[0]) => ({})),
	};
	const log = mock(async (_: Parameters<Client["app"]["log"]>[0]) => ({}));
	const client = { session, app: { log }, tui: { showToast: mock(async () => ({})) } };
	const hooks = await RetryDatabaseLockedPlugin({ client } as unknown as PluginInput);
	const remove = () => hooks.event!({ event: {
		type: "session.deleted", properties: { info: { id: "session" } as never },
	} });
	cleanup.push(remove);
	const error = async (message = "database is locked") => {
		await hooks.event!({ event: { type: "session.error", properties: {
			sessionID: "session", error: { name: "UnknownError", data: { message } },
		} } });
		await flush();
	};
	const tick = async (index = waits.length - 1) => { waits[index].resolve(); await flush(); };
	return { session, parent, failed, log, error, remove, tick };
}

test("resumes with the original agent and model variant, fetching only the latest message", async () => {
	const { session, parent, error, tick } = await setup();
	await error();
	expect(session.promptAsync).not.toHaveBeenCalled();
	await tick();
	expect(session.messages.mock.calls).toEqual(Array(2).fill([{
		path: { id: "session" }, query: { limit: 1 }, throwOnError: true,
	}]));
	expect(session.promptAsync).toHaveBeenCalledWith({
		path: { id: "session" }, throwOnError: true,
		body: { agent: parent.agent, model: parent.model, parts: [{ type: "text", text: "continue" }] },
	});
});

test("a newer message cancels recovery even when timestamps match", async () => {
	const { session, failed, error, tick } = await setup();
	await error();
	session.messages.mockResolvedValue({ data: [{ info: { ...failed, id: "newer" } }] });
	await tick();
	expect(session.promptAsync).not.toHaveBeenCalled();
});

test("resumes a user message without fetching another parent", async () => {
	const { session, parent, error, tick } = await setup();
	session.messages.mockResolvedValue({ data: [{ info: parent }] });
	await error();
	await tick();
	expect(session.message).not.toHaveBeenCalled();
	expect(session.promptAsync).toHaveBeenCalledTimes(1);
});

test("repeated errors cancel the older pending retry", async () => {
	const { session, error, tick } = await setup();
	await error();
	await error("SQLITE_BUSY");
	expect(waits[0].signal.aborted).toBe(true);
	await tick();
	expect(session.promptAsync).toHaveBeenCalledTimes(1);
});

test("deleting the session cancels its pending timer", async () => {
	const { session, error, remove } = await setup();
	await error();
	await remove();
	await flush();
	expect(waits[0].signal.aborted).toBe(true);
	expect(session.promptAsync).not.toHaveBeenCalled();
});

test("cancellation during the final API read cannot enqueue a prompt", async () => {
	const { session, failed, error, tick, remove } = await setup();
	await error();
	let resolve!: (value: { data: Array<{ info: typeof failed }> }) => void;
	session.messages.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
	await tick();
	await remove();
	resolve({ data: [{ info: failed }] });
	await flush();
	expect(session.promptAsync).not.toHaveBeenCalled();
});

for (const target of ["messages", "message"] as const) {
	test(`${target} failure never falls back to the default model`, async () => {
		const { session, log, error } = await setup();
		session[target].mockRejectedValue(new Error("API unavailable"));
		await error();
		expect(waits).toHaveLength(0);
		expect(session.promptAsync).not.toHaveBeenCalled();
		expect(log.mock.calls[0][0].body).toMatchObject({ level: "error", message: "database lock recovery failed" });
	});
}

test("missing history is an error, not permission to send an unconfigured continue", async () => {
	const { session, log, error } = await setup();
	session.messages.mockResolvedValue({ data: [] });
	await error();
	expect(session.promptAsync).not.toHaveBeenCalled();
	expect(log.mock.calls[0][0].body.extra.error).toContain("no message to resume");
});

test("a failed final read is retried before sending anything", async () => {
	const { session, error, tick } = await setup();
	await error();
	session.messages.mockRejectedValueOnce(new Error("database is locked"));
	await tick();
	expect(session.promptAsync).not.toHaveBeenCalled();
	await tick();
	expect(session.promptAsync).toHaveBeenCalledTimes(1);
});

test("failed submissions retry with the same model settings", async () => {
	const { session, parent, error, tick } = await setup();
	await error();
	session.promptAsync.mockRejectedValueOnce(new Error("database is locked"));
	await tick();
	await tick();
	expect(session.promptAsync).toHaveBeenCalledTimes(2);
	expect(session.promptAsync.mock.calls.map(([input]) => input.body.model)).toEqual([parent.model, parent.model]);
});

test("plugin instances do not share retry state", async () => {
	const first = await setup();
	const second = await setup();
	await first.error();
	await second.error();
	expect(waits[0].signal.aborted).toBe(false);
	await first.tick(0);
	await second.tick(1);
	expect(first.session.promptAsync).toHaveBeenCalledTimes(1);
	expect(second.session.promptAsync).toHaveBeenCalledTimes(1);
});

test("other errors stay with OpenCode's native retry policy", async () => {
	const { session, error } = await setup();
	await error("service unavailable");
	expect(session.messages).not.toHaveBeenCalled();
	expect(waits).toHaveLength(0);
});

test("structured API errors retain their diagnostic fields", async () => {
	const { session, log, error } = await setup();
	const failure = { name: "UnknownError", data: { message: "database is locked" } };
	session.message.mockRejectedValue(failure);
	await error();
	expect(log.mock.calls[0][0].body.extra.error).toEqual(failure);
});

test("a logging failure after successful submission never submits twice", async () => {
	const { session, log, error, tick } = await setup();
	await error();
	log.mockRejectedValueOnce(new Error("logger unavailable"));
	await tick();
	expect(session.promptAsync).toHaveBeenCalledTimes(1);
	expect(waits).toHaveLength(1);
});
