import type { Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { join } from "node:path";

export default (async ({ client }) => ({
  config: async (config) => {
    const mcp = config.mcp?.composio;
    if (!mcp || !("url" in mcp) || mcp.enabled === false) return;
    if (!mcp.url.startsWith("https://backend.composio.dev/tool_router/")) return;

    // Reuse the CLI login instead of storing a credential in synced config.
    const path = join(homedir(), ".composio/user_data.json");
    let auth: unknown;
    try {
      auth = await Bun.file(path).json();
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      mcp.enabled = false;
      await client.app.log({
        body: {
          service: "composio-auth",
          level: "warn",
          message: `Composio MCP disabled: no CLI login at ${path}. Authenticate with the Composio CLI.`,
        },
      });
      return;
    }
    const key = auth && typeof auth === "object" && "api_key" in auth ? auth.api_key : undefined;
    if (typeof key !== "string" || !key.startsWith("uak_")) {
      throw new Error(`Invalid Composio CLI login at ${path}: expected a user API key (uak_).`);
    }
    mcp.headers = { ...mcp.headers, "x-user-api-key": key };
  },
})) satisfies Plugin;
