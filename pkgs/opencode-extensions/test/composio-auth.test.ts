import { afterEach, beforeEach, expect, mock, spyOn, test } from "bun:test";
import type { Config, PluginInput } from "@opencode-ai/plugin";
import composioAuth from "../plugins/composio-auth";

const json = mock(async (): Promise<unknown> => ({ api_key: "uak_test" }));
const log = mock(async () => ({}));
let file: ReturnType<typeof spyOn>;
const hooks = await composioAuth({ client: { app: { log } } } as unknown as PluginInput);
const server = () => ({
  type: "remote" as const,
  url: "https://backend.composio.dev/tool_router/trs_test/mcp",
  headers: { "x-org-id": "org_test", "x-project-id": "proj_test" },
});

beforeEach(() => {
  json.mockReset().mockResolvedValue({ api_key: "uak_test" });
  log.mockClear();
  file = spyOn(Bun, "file").mockReturnValue({ json } as unknown as ReturnType<typeof Bun.file>);
});
afterEach(() => file.mockRestore());

test("injects the CLI key while preserving session scope", async () => {
  const config: Config = { mcp: { composio: server() } };
  await hooks.config(config);
  expect(config.mcp?.composio).toEqual({
    ...server(),
    headers: { ...server().headers, "x-user-api-key": "uak_test" },
  });
  expect(log).not.toHaveBeenCalled();
});

test.each([
  undefined,
  { enabled: false },
  { ...server(), enabled: false },
  { type: "local", command: ["example"] },
  { ...server(), url: "https://connect.composio.dev/mcp" },
  { ...server(), url: "https://backend.composio.dev.evil.test/tool_router/trs_test/mcp" },
  { ...server(), url: "https://backend.composio.dev@evil.test/tool_router/trs_test/mcp" },
] satisfies (NonNullable<Config["mcp"]>[string] | undefined)[])("does not read credentials for an unrelated or disabled server: %j", async (composio) => {
  const config: Config = composio ? { mcp: { composio } } : {};
  const before = structuredClone(config);
  await hooks.config(config);
  expect(config).toEqual(before);
  expect(file).not.toHaveBeenCalled();
});

test("a missing login disables only Composio and logs an actionable warning", async () => {
  json.mockRejectedValue(Object.assign(new Error("missing"), { code: "ENOENT" }));
  const config: Config = { mcp: { composio: server(), other: server() } };
  await hooks.config(config);
  expect(config.mcp?.composio).toEqual({ ...server(), enabled: false });
  expect(config.mcp?.other).toEqual(server());
  expect(log).toHaveBeenCalledTimes(1);
  expect(log).toHaveBeenCalledWith({ body: {
    service: "composio-auth", level: "warn",
    message: expect.stringContaining("Authenticate with the Composio CLI"),
  } });
});

test.each([new SyntaxError("invalid JSON"), Object.assign(new Error("permission denied"), { code: "EACCES" })])(
  "surfaces a broken login instead of silently disabling Composio: %s", async (error) => {
    json.mockRejectedValue(error);
    const config = { mcp: { composio: server() } };
    await expect(hooks.config(config)).rejects.toBe(error);
    expect(config.mcp.composio).toEqual(server());
    expect(log).not.toHaveBeenCalled();
  },
);

test.each([null, {}, [], "login", { api_key: 42 }, { api_key: "ck_secret" }])(
  "rejects invalid credential shapes with a useful, secret-free error: %j", async (auth) => {
    json.mockResolvedValue(auth);
    await expect(hooks.config({ mcp: { composio: server() } })).rejects.toThrow(
      "expected a user API key (uak_)",
    );
  },
);
