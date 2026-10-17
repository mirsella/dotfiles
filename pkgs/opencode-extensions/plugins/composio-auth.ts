import { applyCredentials } from "../lib/composio-auth";
import { Plugin } from "@opencode/plugin";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

export default Plugin.define({
  id: "local.composio-auth",
  async setup(ctx) {
    const path = join(homedir(), ".composio/user_data.json");
    let key: string | undefined;
    try {
      const auth = JSON.parse(await readFile(path, "utf8"));
      if (
        typeof auth?.api_key !== "string" ||
        !auth.api_key.startsWith("uak_") ||
        auth.api_key.length <= 4
      ) {
        throw new Error(
          `Invalid Composio CLI login at ${path}: expected a user API key (uak_).`,
        );
      }
      key = auth.api_key;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    await ctx.mcp.transform((editor) => {
      if (editor.get("composio"))
        editor.update("composio", (server) => applyCredentials(server, key));
    });
    if (!key)
      console.warn(
        `Composio MCP disabled without a CLI login at ${path}. Authenticate with the Composio CLI.`,
      );
  },
});
