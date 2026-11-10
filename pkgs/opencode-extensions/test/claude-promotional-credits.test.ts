import { expect, test } from "bun:test";
import { creditEndpoint, parseCreditStatus } from "../lib/claude-promotional-credits";
import plugin from "../plugins/claude-promotional-credits";
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

test("subscription restores the exact provider; no session/auth/retry hooks exist", async () => {
  const directory = mkdtempSync(join(tmpdir(), "claude-credit-plugin-"));
  mkdirSync(join(directory, "claude-credit-router"));
  writeFileSync(join(directory, "claude-credit-router/config.json"), JSON.stringify({ listen: "127.0.0.1:17322", upstream, organization_id: "personal" }));
  const originalHome = process.env.XDG_CONFIG_HOME;
  const originalFetch = globalThis.fetch;
  const originalInterval = globalThis.setInterval;
  const originalError = console.error;
  let tick: () => void = () => { throw new Error("interval not registered"); };
  let status: unknown = { route: "promotional", organization_id: "personal" };
  let transform: (editor: any) => void = () => {};
  let onReload = () => {};
  const source = {
    "claude-agent": { settings: { baseURL: upstream, chunkTimeout: 180_000 }, nativeAuth: "untouched", models: ["opus#high"] },
    openai: { settings: { baseURL: "native-codex" }, giftedCredits: "untouched" },
  };
  let current = structuredClone(source);
  let cleanup: (() => void) | void;
  try {
    process.env.XDG_CONFIG_HOME = directory;
    globalThis.fetch = (async () => Response.json(status)) as any;
    globalThis.setInterval = ((callback: () => void) => {
      tick = callback;
      return originalInterval(() => {}, 1_000_000);
    }) as any;
    console.error = () => {};
    cleanup = await plugin.setup({ provider: {
      transform: async (handler: typeof transform) => { transform = handler; },
      reload: async () => {
        current = structuredClone(source);
        transform({
          get: (id: keyof typeof source) => ({ provider: current[id] }),
          update: (id: keyof typeof source, update: (provider: unknown) => void) => update(current[id]),
        });
        onReload();
      },
    } } as any);
    expect(current["claude-agent"].settings.baseURL).toBe(router);
    expect(current.openai).toEqual(source.openai);
    const reload = async (value: unknown) => {
      status = value;
      const done = new Promise<void>((resolve) => { onReload = resolve; });
      tick();
      await done;
    };
    await reload({ route: "subscription", organization_id: "personal" });
    expect(current).toEqual(source);
    await reload({ route: "promotional", organization_id: "personal" });
    expect(current["claude-agent"].settings.baseURL).toBe(router);
    await reload({ route: "promotional", organization_id: "company" });
    expect(current).toEqual(source);
  } finally {
    cleanup?.();
    globalThis.fetch = originalFetch;
    globalThis.setInterval = originalInterval;
    console.error = originalError;
    if (originalHome === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = originalHome;
    rmSync(directory, { recursive: true });
  }
});
