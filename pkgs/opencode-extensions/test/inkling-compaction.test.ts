import { expect, test } from "bun:test";
import plugin from "../plugins/inkling-compaction";

const localURL = "http://127.0.0.1:8080/v1/chat/completions";

async function fixture() {
  const handlers: Record<string, (event: any) => Promise<void>> = {};
  const hook = async (name: string, callback: (event: any) => Promise<void>, scope: object) => {
    expect(scope).toEqual({ providerID: "llama.cpp" });
    handlers[name] = callback;
  };
  await plugin.setup({ session: { hook } } as any);
  return handlers;
}

function request(url = localURL) {
  const body = JSON.stringify({ messages: [{ role: "user", content: "Summarize this." }], max_tokens: 8192 });
  return new Request(url, {
    method: "POST",
    headers: { "content-type": "application/json", "content-length": String(body.length) },
    body,
  });
}

test("local Inkling compaction caps reasoning without changing the prompt or output limit", async () => {
  const original = request();
  const event = { kind: "compaction", model: { id: "inkling-2-mini-uncensored" }, request: original };
  await (await fixture())["http.request"](event);
  expect(event.request.headers.get("content-length")).toBeNull();
  expect(await event.request.json()).toEqual({ ...await original.json(), reasoning_budget_tokens: 512 });
});

test("normal replies, other models and other endpoints keep the original request", async () => {
  const apply = (await fixture())["http.request"];
  for (const [kind, id, url] of [
    ["primary", "inkling-2-mini-uncensored", localURL],
    ["compaction", "another-model", localURL],
    ["compaction", "inkling-2-mini-uncensored", "http://127.0.0.1:17321/v1/chat/completions"],
  ]) {
    const original = request(url);
    const event = { kind, model: { id }, request: original };
    await apply(event);
    expect(event.request).toBe(original);
    expect(original.bodyUsed).toBe(false);
  }
});

function context() {
  const sources = [
    { key: "core/codemode", text: "Code Mode catalog\n\nIncludes T3 functions" },
    { key: "core/mcp-guidance", text: "MCP instructions" },
    { key: "core/skill-guidance", text: "Skill catalog" },
    { key: "project/agents", text: "Use the checked-in test command." },
  ];
  const names = ["read", "glob", "grep", "edit", "write", "shell", "websearch", "webfetch",
    "execute", "t3_thread_send", "browser", "computer_use", "subagent", "skill", "exa_web_search_exa"];
  const result = { type: "tool-result", id: "call_1", name: "shell",
    result: { type: "text", value: `START${"x".repeat(18_000)}END` } };
  return {
    model: { providerID: "llama.cpp", id: "inkling-2-mini-uncensored" },
    tools: Object.fromEntries(names.map((name) => [name, { description: name, input: {} }])),
    system: [{ text: sources.map((source) => source.text).join("\n\n"), metadata: { experimental: { instructions: sources } } }],
    messages: [{ role: "tool", content: [result] }],
  };
}

test("Local keeps coding and web tools by default, project rules, and excerpts of large tool output", async () => {
  const event = context();
  const stored = event.messages[0].content[0];
  await (await fixture()).context(event);
  expect(Object.keys(event.tools)).toEqual(["read", "glob", "grep", "edit", "write", "shell", "websearch", "webfetch"]);
  expect(event.system.map((part) => part.text)).toEqual(["Use the checked-in test command."]);
  const result = event.messages[0].content[0];
  expect(result.id).toBe(stored.id);
  expect(result.result.value).toStartWith("START");
  expect(result.result.value).toEndWith("END");
  expect(result.result.value).toContain("6008 characters omitted");
  expect(stored.result.value).toHaveLength(18_008);
});

test("summaries have no tools; cloud contexts are untouched", async () => {
  const handlers = await fixture();
  const summary = context();
  await handlers.compaction(summary);
  expect(summary.tools).toEqual({});
  expect(summary.system.map((part) => part.text)).toEqual(["Use the checked-in test command."]);
  const cloud = context();
  cloud.model.providerID = "openai";
  const original = structuredClone(cloud);
  await handlers.context(cloud);
  expect(cloud).toEqual(original);
});

test("Local preserves mixed historical instructions but clarifies which tools are available", async () => {
  const event = context();
  const notice = { role: "system", content: [{ type: "text", text: "Use execute. Run project checks before editing." }] };
  const withHistory = { ...event, messages: [notice, ...event.messages] };
  await (await fixture()).context(withHistory);
  expect(withHistory.messages[0]).toEqual(notice);
  expect(withHistory.tools).not.toHaveProperty("execute");
  expect(withHistory.system.at(-1)?.text).toContain("only tools available in this request");
});
