import { Plugin } from "@opencode/plugin";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { creditEndpoint, parseCreditStatus } from "./claude-promotional-credits";

export function creditRoutingPlugin(vendor: "claude" | "openai", providerID: string) {
  return Plugin.define({
    id: `local.${vendor}-promotional-credits`,
    async setup(ctx) {
      const path = join(process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"), `${vendor}-credit-router/config.json`);
      if (!existsSync(path)) return;
      const config = JSON.parse(readFileSync(path, "utf8"));
      if (!/^127\.0\.0\.1:\d+$/.test(config.listen) || typeof config.upstream !== "string" || typeof config.organization_id !== "string") {
        throw new Error(`Invalid local ${vendor} credit router configuration`);
      }
      const endpoint = `http://${config.listen}`;
      const controller = new AbortController();
      let promotional = false;
      let busy = false;
      let warning = "";
      const warn = (message: string) => {
        if (message !== warning) console.error(`[${vendor}-promotional-credits] ${message}`);
        warning = message;
      };
      const refresh = async () => {
        if (busy || controller.signal.aborted) return;
        busy = true;
        let next = false;
        try {
          const response = await fetch(`${endpoint}/_credit/status`, {
            signal: AbortSignal.any([controller.signal, AbortSignal.timeout(2_000)]), cache: "no-store",
          });
          if (!response.ok) throw new Error(`router status ${response.status}`);
          next = parseCreditStatus(await response.json(), config.organization_id).route === "promotional";
          warning = "";
        } catch (error) {
          if (!controller.signal.aborted) warn(`Router unavailable; retaining native subscription behavior: ${String(error)}`);
        } finally { busy = false; }
        if (controller.signal.aborted || next === promotional) return;
        const previous = promotional;
        promotional = next;
        try { await ctx.provider.reload(); }
        catch (error) { promotional = previous; throw error; }
      };
      await ctx.provider.transform((editor) => {
        if (!promotional) return;
        const source = editor.get(providerID);
        if (!source) return;
        try {
          const baseURL = creditEndpoint(source.provider.settings, config.upstream, endpoint);
          editor.update(providerID, (provider) => {
            provider.settings.baseURL = baseURL;
            // API routing uses HTTP; restore the native transport along with
            // the original endpoint as soon as subscription routing takes over.
            if (vendor === "openai") provider.settings.transport = "http";
          });
        } catch (error) { warn(String(error)); }
      });
      await refresh();
      const timer = setInterval(() => { void refresh().catch((error) => warn(String(error))); }, 5_000);
      timer.unref();
      return () => { controller.abort(); clearInterval(timer); };
    },
  });
}
