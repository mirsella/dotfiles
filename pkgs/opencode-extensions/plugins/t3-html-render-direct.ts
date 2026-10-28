import { Plugin } from "@opencode/plugin";

// T3 injects one MCP server per thread as `t3-code-<thread-id>`. OpenCode Code
// Mode wraps MCP calls in `execute`, but T3 only renders html_render when the
// recorded tool name resolves to html_render. Re-register the same namespaced
// tool with codemode disabled; the latest registration for an effective name
// wins, and the namespace keeps the override scoped to its thread.
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
        const options = tool.options;
        if (!isT3HtmlRender(tool.id) || !options?.namespace || options.codemode === false) continue;
        tools.add({
          name: tool.name,
          description: `${tool.description}\n\nCall this tool directly; do not route it through execute. Its result is shown to the reader.`.trim(),
          input: tool.input,
          output: tool.output,
          execute: tool.execute,
          options: {
            namespace: options.namespace,
            permission: options.permission,
            codemode: false,
          },
        });
        if (debug) {
          const native = tools.get(tool.id);
          console.error(`[t3-html-render-direct] ${tool.id}:${native?.options?.codemode} -> native`);
        }
      }
    });

    return () => registration.dispose();
  },
});
