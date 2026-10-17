import { expect, spyOn, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import plugin from "../tui-plugins/cache-timer-tui";

test("cache refresh skips cold, busy, and interactive sessions", async () => {
  const root = mkdtempSync(join(tmpdir(), "cache-timer-"));
  const original = process.env.OPENCODE_CONFIG_DIR;
  process.env.OPENCODE_CONFIG_DIR = root;
  writeFileSync(join(root, "cache-timer.json"), JSON.stringify({
    enableAutoPrompt: true,
    enabledProviders: ["openai"],
    defaultDuration: 300,
  }));
  let tick = () => {};
  const timer = spyOn(globalThis, "setInterval").mockImplementation((callback) => {
    tick = callback as () => void;
    return 0 as never;
  });
  const clear = spyOn(globalThis, "clearInterval").mockImplementation(() => {});
  let status = "idle";
  let interactive = false;
  let age = 310;
  let requests = 0;
  let dispose: (() => void) | undefined;
  try {
    dispose = await plugin.setup({
      ui: { slot: () => () => {}, toast: { show: () => {} } },
      data: { session: {
        list: () => [{ id: "test-session" }],
        status: () => status,
        permission: { list: () => interactive ? [{}] : [] },
        form: { list: () => [] },
        message: { list: () => [{
          type: "assistant",
          model: { providerID: "openai", id: "test-model" },
          time: { created: Date.now() - age * 1000 },
        }] },
      } },
      client: { session: { prompt: async () => { requests++; } } },
    } as never) as (() => void) | undefined;
    tick();
    expect(requests).toBe(0);
    age = 30;
    tick();
    expect(requests).toBe(0);
    age = 270;
    status = "running";
    tick();
    expect(requests).toBe(0);
    status = "idle";
    interactive = true;
    tick();
    expect(requests).toBe(0);
    interactive = false;
    tick();
    expect(requests).toBe(1);
    await Promise.resolve();
  } finally {
    dispose?.();
    timer.mockRestore();
    clear.mockRestore();
    if (original === undefined) delete process.env.OPENCODE_CONFIG_DIR;
    else process.env.OPENCODE_CONFIG_DIR = original;
    rmSync(root, { recursive: true });
  }
});
