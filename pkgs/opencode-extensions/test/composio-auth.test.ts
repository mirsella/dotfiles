import { expect, spyOn, test } from "bun:test";
import { applyCredentials } from "../lib/composio-auth";
import plugin from "../plugins/composio-auth";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import * as os from "node:os";
import { join } from "node:path";

test("authentication rejects malformed CLI data and accepts a complete user key", async () => {
  const root = mkdtempSync(join(os.tmpdir(), "composio-auth-"));
  const home = spyOn(os, "homedir").mockReturnValue(root);
  const directory = join(root, ".composio");
  mkdirSync(directory);
  const server = { type: "remote", url: "https://backend.composio.dev/tool_router/abc" };
  const ctx = { mcp: { transform: async (fn: Function) => fn({
    get: () => server,
    update: (_name: string, update: Function) => update(server),
  }) } };
  try {
    for (const auth of [null, {}, { api_key: null }, { api_key: "uak_" }]) {
      writeFileSync(join(directory, "user_data.json"), JSON.stringify(auth));
      await expect(plugin.setup(ctx as never)).rejects.toThrow("expected a user API key");
    }
    writeFileSync(join(directory, "user_data.json"), JSON.stringify({ api_key: "uak_test" }));
    await plugin.setup(ctx as never);
    expect(server).toHaveProperty("headers.x-user-api-key", "uak_test");
  } finally {
    home.mockRestore();
    rmSync(root, { recursive: true });
  }
});

test("credentials preserve routing headers and replace only authentication", () => {
  const server = {
    type: "remote",
    url: "https://backend.composio.dev/tool_router/abc",
    headers: { "x-project-id": "project", "x-user-api-key": "old" },
  };
  applyCredentials(server, "uak_test");
  expect(server.headers).toEqual({
    "x-project-id": "project",
    "x-user-api-key": "uak_test",
  });
});
test("missing login disables the Composio router", () => {
  const server = {
    type: "remote",
    url: "https://backend.composio.dev/tool_router/abc",
  };
  applyCredentials(server);
  expect(server).toEqual({ type: "remote", url: server.url, disabled: true });
});
test("unrelated origins and explicit disablement stay untouched", () => {
  for (const server of [
    {
      type: "remote",
      url: "https://backend.composio.dev.example/tool_router/abc",
    },
    {
      type: "remote",
      url: "https://backend.composio.dev/tool_router/abc",
      disabled: true,
    },
    { type: "local", url: undefined },
  ]) {
    const original = structuredClone(server);
    applyCredentials(server, "uak_test");
    expect(server).toEqual(original);
  }
});
