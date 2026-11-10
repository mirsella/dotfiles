import { expect, test } from "bun:test";
import { creditEndpoint, parseCreditStatus } from "../lib/claude-promotional-credits";
import plugin from "../plugins/claude-promotional-credits";
import openaiPlugin from "../plugins/openai-promotional-credits";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const upstream = "http://127.0.0.1:17321/sleev/opencode/anthropic";
const router = "http://127.0.0.1:17322";

test("router status must identify the configured personal organization", () => {
  expect(parseCreditStatus({ route: "promotional", organization_id: "personal" }, "personal").route).toBe("promotional");
  expect(parseCreditStatus({ route: "subscription", organization_id: "personal" }, "personal").route).toBe("subscription");
  for (const value of [null, {}, { route: "promotional", organization_id: "company" }, { route: "other", organization_id: "personal" }]) {
    expect(() => parseCreditStatus(value, "personal")).toThrow();
  }
});

test("only the existing Claude subscription endpoint can be wrapped", () => {
  const settings = { baseURL: upstream, chunkTimeout: 180_000 };
  expect(creditEndpoint(settings, upstream, router)).toBe(router);
  expect(settings).toEqual({ baseURL: upstream, chunkTimeout: 180_000 });
  for (const value of [{ baseURL: "https://other.example" }, { baseURL: upstream, apiKey: "key" }, { baseURL: upstream, authToken: "token" }]) {
    expect(() => creditEndpoint(value, upstream, router)).toThrow("leaving provider unchanged");
  }
});

for (const vendor of ["claude", "openai"]) test(`${vendor}: subscription restores the exact provider; no auth/retry hooks exist`, async () => {
  const openai = vendor === "openai";
  const upstream = openai ? "http://127.0.0.1:17321/sleev/opencode/codex" : "http://127.0.0.1:17321/sleev/opencode/anthropic";
  const directory = mkdtempSync(join(tmpdir(), "claude-credit-plugin-"));
  mkdirSync(join(directory, `${vendor}-credit-router`));
  writeFileSync(join(directory, `${vendor}-credit-router/config.json`), JSON.stringify({ listen: "127.0.0.1:17322", upstream, organization_id: "personal" }));
  const originalHome = process.env.XDG_CONFIG_HOME;
  const originalState = process.env.XDG_STATE_HOME;
  const originalFetch = globalThis.fetch;
  const originalInterval = globalThis.setInterval;
  const originalError = console.error;
  let tick: () => void = () => { throw new Error("interval not registered"); };
  let status: unknown = { route: "promotional", organization_id: "personal" };
  type Transform = (editor: any) => void;
  const transforms = new Set<Transform>();
  let emitActivation = () => { throw new Error("event subscription not registered"); };
  let onRegistration = () => {};
  let onReload = () => {};
  const source = {
    "claude-agent": { settings: { baseURL: openai ? "native-claude" : upstream, chunkTimeout: 180_000 }, nativeAuth: "untouched", models: ["opus#high"] },
    openai: { settings: { baseURL: openai ? upstream : "native-codex", transport: "websocket" }, giftedCredits: "untouched" },
  };
  let current = structuredClone(source);
  let cleanup: (() => void) | void;
  try {
    process.env.XDG_CONFIG_HOME = directory;
    process.env.XDG_STATE_HOME = join(directory, "state");
    globalThis.fetch = (async () => Response.json(status)) as any;
    globalThis.setInterval = ((callback: () => void) => {
      tick = callback;
      return originalInterval(() => {}, 1_000_000);
    }) as any;
    console.error = () => {};
    const render = () => {
        current = structuredClone(source);
        for (const transform of transforms) transform({
          get: (id: keyof typeof source) => ({ provider: current[id] }),
          update: (id: keyof typeof source, update: (provider: unknown) => void) => update(current[id]),
        });
        onReload();
    };
    cleanup = await (openai ? openaiPlugin : plugin).setup({
      location: { directory },
      session: { hook: async (name: string) => { expect(name).toBe("model.request"); } },
      event: { subscribe: ({ signal }: { signal: AbortSignal }) => ({
        [Symbol.asyncIterator]: () => ({ next: () => new Promise((resolve) => {
          const aborted = () => resolve({ done: true });
          signal.addEventListener("abort", aborted, { once: true });
          emitActivation = () => {
            signal.removeEventListener("abort", aborted);
            resolve({ done: false, value: { type: "plugin.updated" } });
          };
        }) }),
      }) },
      provider: {
        transform: async (handler: Transform) => {
          transforms.add(handler);
          render();
          onRegistration();
          return { dispose: async () => { transforms.delete(handler); render(); } };
        },
        reload: async () => render(),
      },
    } as any);
    const expectPromotional = () => {
      if (openai) {
        expect(current.openai.settings).toEqual({ baseURL: router, transport: "http" });
        expect(current.openai.giftedCredits).toBe("untouched");
        expect(current["claude-agent"]).toEqual(source["claude-agent"]);
      } else {
        expect(current["claude-agent"].settings.baseURL).toBe(router);
        expect(current.openai).toEqual(source.openai);
      }
    };
    expectPromotional();
    // The real config plugin registers after external plugins. Its overlay
    // must not silently undo routing after activation, or on later reloads.
    transforms.add((editor) => editor.update(openai ? "openai" : "claude-agent", (provider: any) => {
      provider.settings.baseURL = upstream;
      if (openai) provider.settings.transport = "websocket";
    }));
    render();
    expect(current).toEqual(source);
    const activated = new Promise<void>((resolve) => { onRegistration = resolve; });
    emitActivation();
    await activated;
    expectPromotional();
    const reload = async (value: unknown) => {
      status = value;
      const done = new Promise<void>((resolve) => { onReload = resolve; });
      tick();
      await done;
    };
    await reload({ route: "subscription", organization_id: "personal" });
    expect(current).toEqual(source);
    await reload({ route: "promotional", organization_id: "personal" });
    expectPromotional();
    await reload({ route: "promotional", organization_id: "company" });
    expect(current).toEqual(source);
  } finally {
    cleanup?.();
    globalThis.fetch = originalFetch;
    globalThis.setInterval = originalInterval;
    console.error = originalError;
    if (originalHome === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = originalHome;
    if (originalState === undefined) delete process.env.XDG_STATE_HOME;
    else process.env.XDG_STATE_HOME = originalState;
    rmSync(directory, { recursive: true });
  }
});
