import type { Plugin } from "@opencode/plugin";
import { createHash, randomUUID } from "node:crypto";
import { appendFileSync, closeSync, constants, fstatSync, fsyncSync, mkdirSync, openSync } from "node:fs";
import { homedir, hostname } from "node:os";
import { dirname, join } from "node:path";

type ObjectData = Record<string, unknown>;
const object = (value: unknown): ObjectData =>
  value !== null && typeof value === "object" && !Array.isArray(value) ? value as ObjectData : {};
const number = (value: unknown) =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : undefined;
const id = (value: unknown) =>
  typeof value === "string" && value.length > 0 && value.length <= 200 &&
  !value.startsWith("sk-") && /^[\w.:/\-]+$/.test(value) ? value : undefined;
const priceCache = new Map<string, { at: number; value: Promise<{ models: ObjectData; at: string }> }>();
const tokens = (value: unknown) => {
  const usage = object(value);
  const cache = object(usage.cache);
  if ([usage.input, usage.output, usage.reasoning, cache.read, cache.write].some(value => !Number.isSafeInteger(value) || Number(value) < 0)) return undefined;
  return { input: usage.input, output: usage.output, reasoning: usage.reasoning, cache_read: cache.read, cache_write: cache.write };
};

/** Explicit metadata allowlist. Never spread native events or provider payloads. */
export function usageRecord(value: unknown, providerID: string, known: ObjectData = {}) {
  const event = object(value);
  const data = object(event.data);
  const model = object(data.model);
  if (!id(data.sessionID) || !id(event.id)) return;
  if (model.providerID !== undefined && model.providerID !== providerID) return;
  const context = {
    parent_session_id: id(known.parent_session_id),
    agent: id(known.agent), kind: id(known.kind),
    session_id: id(data.sessionID), message_id: id(data.assistantMessageID),
    model: id(model.id) ?? id(known.model), native_session_id: id(object(data.native).sessionID),
  };
  if (["session.tool.called", "session.text.ended", "session.reasoning.ended"].includes(String(event.type)) && context.native_session_id) {
    return {
      id: `opencode:${event.id}`, at: new Date(Number(event.created)).toISOString(),
      event: "opencode_binding", source: "opencode", machine: hostname(), context, route: null,
    };
  }
  if (event.type !== "session.step.ended" && event.type !== "session.step.failed" && event.type !== "session.compaction.ended") return;
  const usage = tokens(data.tokens);
  return {
    id: `opencode:${event.id}`, at: new Date(Number(event.created)).toISOString(),
    event: "opencode_usage", source: "opencode", machine: hostname(), context,
    outcome: event.type === "session.step.failed" ? `failed:${id(object(data.error).type) ?? "unknown"}` : "complete", tokens: usage,
    opencode_cost_usd: number(data.cost), route: null, reason: "passive_usage_not_vendor_billing",
  };
}

export async function creditAudit(ctx: Plugin.Context, providerID: string, vendor: "openai" | "claude", endpoint: string, signal: AbortSignal, warn: (message: string) => void) {
  const directory = join(process.env.XDG_STATE_HOME ?? join(homedir(), ".local/state"), "credit-router", `${vendor}-credit-router`, "audit-spool");
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const parent = openSync(dirname(directory), constants.O_RDONLY | constants.O_DIRECTORY);
  try { fsyncSync(parent); } finally { closeSync(parent); }
  const location = createHash("sha256").update(ctx.location.directory).digest("hex").slice(0, 16);
  const open = () => {
    const fd = openSync(join(directory, `opencode-${process.pid}-${location}-${randomUUID()}.jsonl`), constants.O_WRONLY | constants.O_APPEND | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
    const stat = fstatSync(fd);
    if (!stat.isFile() || (stat.mode & 0o077) !== 0 || stat.uid !== process.getuid?.()) {
      closeSync(fd);
      throw new Error("Audit spool must be a private owned regular file");
    }
    const parent = openSync(directory, constants.O_RDONLY | constants.O_DIRECTORY);
    try { fsyncSync(parent); } catch (error) { closeSync(fd); throw error; } finally { closeSync(parent); }
    return fd;
  };
  let fd: number | undefined = open();
  let closed = false;
  signal.addEventListener("abort", () => {
    closed = true;
    if (fd !== undefined) closeSync(fd);
    fd = undefined;
  }, { once: true });
  const write = (record: ObjectData) => {
    if (closed) return;
    try {
      const line = JSON.stringify(record) + "\n";
      if (Buffer.byteLength(line) > 65_536) throw new Error("metadata record exceeds bound");
      fd ??= open();
      appendFileSync(fd, line);
      fsyncSync(fd);
    } catch (error) {
      if (fd !== undefined) closeSync(fd);
      fd = undefined;
      warn(`AUDIT WRITE FAILED: ${String(error)}; next record uses a fresh spool`);
    }
  };
  const steps = new Map<string, ObjectData>();
  const sessions = new Map<string, ObjectData>();
  let prices: ObjectData = {};
  // Public API rates, never the zero-priced subscription catalogue. Failure
  // means unknown cost, not zero. The source timestamp is retained per record.
  let fetchedAt = new Date().toISOString(), refreshAt = 0;
  const rates = (model: unknown) => {
    if (typeof model !== "string") return undefined;
    const cost = object(object(prices[model]).cost);
    if (number(cost.input) === undefined || number(cost.output) === undefined) return undefined;
    const values = (value: ObjectData) => ({ input: number(value.input), output: number(value.output), cache_read: number(value.cache_read), cache_write: number(value.cache_write) });
    const tiers = (Array.isArray(cost.tiers) ? cost.tiers : [])
      .map(value => object(value))
      .filter(value => number(object(value.tier).size) !== undefined &&
        number(value.input) !== undefined && number(value.output) !== undefined)
      .map(value => ({ above: number(object(value.tier).size), ...values(value) }));
    return { ...values(cost), fetched_at: fetchedAt, tiers };
  };
  const refreshPrices = () => {
    if (Date.now() < refreshAt) return;
    refreshAt = Date.now() + 24 * 60 * 60 * 1000;
    let cache = priceCache.get(vendor);
    if (!cache || Date.now() - cache.at > 24 * 60 * 60 * 1000) {
      cache = { at: Date.now(), value: fetch("https://models.dev/api.json", { signal: AbortSignal.timeout(5_000) }).then(async response => {
        if (!response.ok) throw new Error(`pricing HTTP ${response.status}`);
        return { models: object(object(object(await response.json())[vendor === "claude" ? "anthropic" : "openai"]).models), at: new Date().toISOString() };
      }) };
      priceCache.set(vendor, cache);
    }
    const requested = cache;
    void requested.value.then(value => {
      prices = value.models;
      fetchedAt = value.at;
      for (const model of Object.keys(prices)) {
        const rate = rates(model);
        if (rate && id(model)) write({ id: `pricing:${vendor}:${fetchedAt}:${model}`, at: fetchedAt, event: "pricing", source: "opencode", machine: hostname(), context: { model }, rates: rate });
      }
    }).catch(error => {
      if (priceCache.get(vendor) === requested) priceCache.delete(vendor);
      refreshAt = Date.now() + 60 * 60 * 1000;
      if (!signal.aborted) warn(`API price estimates unavailable: ${String(error)}`);
    });
  };
  refreshPrices();
  await ctx.session.hook("model.request", async event => {
    if (event.model.providerID !== providerID) return;
    refreshPrices();
    let parent = sessions.get(event.sessionID);
    if (!parent) {
      try {
        const session = await ctx.session.get({ sessionID: event.sessionID });
        parent = { parent_session_id: id(session.parentID) };
      }
      catch { parent = {}; }
      sessions.set(event.sessionID, parent);
      if (sessions.size > 1024) sessions.delete(sessions.keys().next().value!);
    }
    const context = { ...parent, request_id: randomUUID(), session_id: id(event.sessionID), model: id(event.model.id), agent: id(event.agent), kind: id(event.kind) };
    // Only direct HTTP to our OpenAI router transports correlation headers.
    // Claude Agent owns its HTTP and ignores arbitrary request headers.
    if (vendor === "openai" && event.baseURL?.replace(/\/$/, "") === endpoint) {
      event.headers["x-credit-request-id"] = context.request_id;
      if (context.session_id) event.headers["x-credit-session-id"] = context.session_id;
      if (context.agent) event.headers["x-credit-agent"] = context.agent;
      if (context.kind) event.headers["x-credit-kind"] = context.kind;
    }
    write({ id: `request:${context.request_id}`, at: new Date().toISOString(), event: "opencode_request", source: "opencode", machine: hostname(), context,
      route: null, reason: event.baseURL?.replace(/\/$/, "") === endpoint ? "prepared_router_endpoint_not_transport_confirmation" : "prepared_native_endpoint_not_transport_confirmation", rates: rates(context.model) });
  });
  return (value: unknown) => {
    const event = object(value);
    const data = object(event.data);
    const model = object(data.model);
    const session = id(data.sessionID);
    const message = id(data.assistantMessageID);
    if (event.type === "session.step.started" && model.providerID === providerID && session && message) {
      steps.set(message, { ...sessions.get(session), session_id: session, message_id: message, model: id(model.id), agent: id(data.agent), kind: "primary" });
      if (steps.size > 2048) steps.delete(steps.keys().next().value!);
    }
    const known = message ? steps.get(message) : event.type === "session.compaction.ended" && model.providerID === providerID ? { ...sessions.get(session ?? ""), kind: "compaction" } : undefined;
    if (!known) return;
    const record = usageRecord(value, providerID, known);
    if (record) {
      write({ ...record, rates: rates(record.context.model) });
      if (message && record.event === "opencode_usage") steps.delete(message);
    }
  };
}
