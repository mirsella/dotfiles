import { expect, spyOn, test } from "bun:test";
import * as fs from "node:fs";
import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { creditAudit, usageRecord } from "../lib/credit-audit";

test("usage metadata excludes conversation, native payloads, identities and error text", () => {
  const record = usageRecord({ id: "evt_one", created: Date.now(), type: "session.step.failed", data: {
    sessionID: "ses_one", assistantMessageID: "msg_one", model: { providerID: "claude-agent", id: "opus" }, cost: 0,
    tokens: { input: 4, output: 5, reasoning: 6, cache: { read: 7, write: 8 } },
    error: { type: "RateLimit", message: "PRIVATE_PAYLOAD" }, text: "PRIVATE_PAYLOAD",
    native: { sessionID: "native-one", messageID: "native-message", content: "PRIVATE_PAYLOAD", credentials: "sk-secret" },
  } }, "claude-agent");
  expect(record?.context.native_session_id).toBe("native-one");
  expect(record?.tokens).toEqual({ input: 4, output: 5, reasoning: 6, cache_read: 7, cache_write: 8 });
  expect(record?.opencode_cost_usd).toBe(0);
  expect(record?.route).toBeNull();
  expect(JSON.stringify(record)).not.toContain("PRIVATE_PAYLOAD");
  expect(JSON.stringify(record)).not.toContain("sk-secret");
  expect(usageRecord({ type: "session.text.ended", id: "evt_two", data: { sessionID: "ses_one", text: "PRIVATE_PAYLOAD" } }, "claude-agent")).toBeUndefined();
  expect(usageRecord({ type: "session.step.ended", id: "evt_two", data: { sessionID: "ses_one", model: { providerID: "openai" } } }, "claude-agent")).toBeUndefined();
});

test("audit hooks only add local OpenAI correlation; native requests stay unchanged and survive restarts", async () => {
  const directory = mkdtempSync(join(tmpdir(), "credit-audit-"));
  const originalState = process.env.XDG_STATE_HOME, originalFetch = globalThis.fetch;
  const controller = new AbortController();
  let hook: (event: any) => Promise<void> = async () => { throw new Error("missing hook"); };
  try {
    process.env.XDG_STATE_HOME = directory;
    globalThis.fetch = (async () => Response.json({ openai: { models: { "gpt-test": { cost: { input: 2, output: 4, cache_read: 1 } } } } })) as any;
    const observe = await creditAudit({ location: { directory }, session: {
      hook: async (name: string, callback: typeof hook) => { expect(name).toBe("model.request"); hook = callback; },
      get: async () => ({ parentID: "ses_parent" }),
    } } as any, "openai", "openai", "http://127.0.0.1:17323", controller.signal, message => { throw new Error(message); });
    const native = { sessionID: "ses_one", agent: "build", model: { providerID: "openai", id: "gpt-test" }, kind: "primary", baseURL: "native", headers: { authorization: "PRIVATE_AUTH" } };
    const before = structuredClone(native);
    await hook(native); expect(native).toEqual(before);
    const api = { ...structuredClone(native), baseURL: "http://127.0.0.1:17323" };
    await hook(api);
    expect(api.headers["x-credit-session-id"]).toBe("ses_one");
    expect(api.headers.authorization).toBe("PRIVATE_AUTH");
    observe({ id: "evt_start", type: "session.step.started", data: { sessionID: "ses_one", assistantMessageID: "msg_one", agent: "build", model: native.model } });
    // A later request must never rewrite the earlier step's identity.
    await hook({ ...api, agent: "研究", model: { ...api.model, id: "gpt-later" }, headers: {} });
    observe({ id: "evt_binding", created: Date.now(), type: "session.tool.called", data: { sessionID: "ses_one", assistantMessageID: "msg_one", native: { sessionID: "native-one" }, input: "PRIVATE_TEXT" } });
    observe({ id: "evt_end", created: Date.now(), type: "session.step.ended", data: { sessionID: "ses_one", assistantMessageID: "msg_one", cost: 0, tokens: { input: 1, output: 2, reasoning: 0, cache: { read: 3, write: 4 } }, native: { text: "PRIVATE_TEXT" } } });
    controller.abort();
    const spool = join(directory, "credit-router/openai-credit-router/audit-spool");
    const path = join(spool, readdirSync(spool)[0]);
    expect(statSync(path).mode & 0o777).toBe(0o600);
    const content = readFileSync(path, "utf8");
    const records = content.trim().split("\n").map(line => JSON.parse(line));
    expect(records.some(record => record.event === "opencode_usage" && record.context.parent_session_id === "ses_parent")).toBe(true);
    const usage = records.find(record => record.event === "opencode_usage");
    expect(usage.context.model).toBe("gpt-test"); expect(usage.context.request_id).toBeUndefined();
    expect(records.some(record => record.event === "opencode_binding" && record.context.native_session_id === "native-one")).toBe(true);
    expect(content).not.toContain("PRIVATE_AUTH"); expect(content).not.toContain("PRIVATE_TEXT");
    expect(records.filter(record => record.event === "opencode_request").length).toBe(3);
  } finally {
    controller.abort(); globalThis.fetch = originalFetch;
    if (originalState === undefined) delete process.env.XDG_STATE_HOME; else process.env.XDG_STATE_HOME = originalState;
    rmSync(directory, { recursive: true, force: true });
  }
});

test("compaction uses the public accounting event and title usage is not invented", () => {
  const event = { id: "evt_compact", created: Date.now(), type: "session.compaction.ended", data: { sessionID: "ses_one", model: { providerID: "openai", id: "gpt-test" }, text: "PRIVATE_TEXT", cost: 1, tokens: { input: 1, output: 2, reasoning: 0, cache: { read: 0, write: 0 } } } };
  expect(usageRecord(event, "openai", { kind: "compaction" })?.tokens?.output).toBe(2);
  expect(usageRecord({ ...event, type: "session.usage.recorded" }, "openai")).toBeUndefined();
  expect(JSON.stringify(usageRecord(event, "openai"))).not.toContain("PRIVATE_TEXT");
});

test("a partial append rotates the spool instead of poisoning later records", async () => {
  const directory = mkdtempSync(join(tmpdir(), "credit-audit-partial-"));
  const originalState = process.env.XDG_STATE_HOME;
  const controller = new AbortController();
  let hook: (event: any) => Promise<void> = async () => {};
  const append = fs.appendFileSync;
  let spy: ReturnType<typeof spyOn> | undefined;
  try {
    process.env.XDG_STATE_HOME = directory;
    await creditAudit({ location: { directory }, session: { hook: async (_: string, fn: typeof hook) => { hook = fn; }, get: async () => ({}) } } as any, "openai", "openai", "router", controller.signal, () => {});
    spy = spyOn(fs, "appendFileSync").mockImplementationOnce((fd: any, line: any) => { append(fd, line.slice(0, 10)); throw new Error("simulated disk full"); });
    const event = { sessionID: "ses_one", agent: "build", model: { providerID: "openai", id: "gpt-test" }, kind: "primary", baseURL: "native", headers: {} };
    await hook(event);
    spy.mockRestore(); spy = undefined;
    await hook(event);
    controller.abort();
    const spool = join(directory, "credit-router/openai-credit-router/audit-spool");
    const contents = readdirSync(spool).map(name => readFileSync(join(spool, name), "utf8"));
    expect(contents.some(text => !text.endsWith("\n"))).toBe(true);
    expect(contents.filter(text => text.endsWith("\n")).flatMap(text => text.trim().split("\n")).some(line => JSON.parse(line).event === "opencode_request")).toBe(true);
  } finally { spy?.mockRestore(); controller.abort(); if (originalState === undefined) delete process.env.XDG_STATE_HOME; else process.env.XDG_STATE_HOME = originalState; rmSync(directory, { recursive: true, force: true }); }
});

test("a failed public pricing fetch is evicted instead of cached for a day", async () => {
  const directory = mkdtempSync(join(tmpdir(), "credit-prices-"));
  const originalState = process.env.XDG_STATE_HOME, originalFetch = globalThis.fetch;
  const controllers = [new AbortController(), new AbortController()];
  const clock = spyOn(Date, "now");
  let calls = 0;
  let failed!: () => void;
  const warning = new Promise<void>(resolve => { failed = resolve; });
  try {
    process.env.XDG_STATE_HOME = directory;
    const now = new Date().getTime() + 2 * 86_400_000;
    clock.mockReturnValue(now);
    globalThis.fetch = (async () => { calls++; return calls === 1 ? new Response("", { status: 500 }) : Response.json({ openai: { models: {} } }); }) as any;
    const ctx = { location: { directory }, session: { hook: async () => {} } } as any;
    await creditAudit(ctx, "openai", "openai", "router", controllers[0].signal, () => failed());
    await warning;
    clock.mockReturnValue(now + 2 * 3_600_000);
    await creditAudit(ctx, "openai", "openai", "router", controllers[1].signal, () => {});
    expect(calls).toBe(2);
  } finally { controllers.forEach(controller => controller.abort()); clock.mockRestore(); globalThis.fetch = originalFetch; if (originalState === undefined) delete process.env.XDG_STATE_HOME; else process.env.XDG_STATE_HOME = originalState; rmSync(directory, { recursive: true, force: true }); }
});
