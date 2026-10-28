import { Plugin } from "@opencode/plugin";

// T3 injects its MCP server once per thread as `t3-code-<thread-id>`. OpenCode
// Code Mode wraps every MCP call in `execute`, but T3 only displays an
// `html_render` result when the recorded tool name is `html_render` itself.
// Keep the rest of the T3 catalog in Code Mode and expose only this tool
// directly, so its result reaches T3's inline page renderer.
const isT3HtmlRender = (id: string) =>
  /^t3-code[-_]/.test(id) && id.endsWith("_html_render");

export default Plugin.define({
  id: "local.t3-html-render-direct",
  async setup(ctx) {
    const debug = process.env.OPENCODE_T3_HTML_RENDER_DEBUG === "1";
    const registration = await ctx.tool.transform((tools) => {
      const candidates = tools.list();
      if (debug) {
        console.error(
          `[t3-html-render-direct] ${candidates
            .map((tool) => `${tool.id}:${tool.options?.codemode}`)
            .join(", ")}`,
        );
      }
      for (const tool of candidates) {
        if (!isT3HtmlRender(tool.id)) continue;
        if (tools.get("html_render")) {
          tools.remove(tool.id);
          continue;
        }
        tools.add({
          name: "html_render",
          description: `${tool.description ?? ""}\n\nCall this native tool directly; do not route it through execute.`.trim(),
          input: tool.input,
          output: tool.output,
          execute: tool.execute,
          options: { codemode: false },
        });
        if (process.env.OPENCODE_T3_HTML_RENDER_DEBUG) {
          console.error(`[t3-html-render-direct] ${tool.id} -> html_render`);
        }
        tools.remove(tool.id);
      }
    });

    return () => registration.dispose();
  },
});
