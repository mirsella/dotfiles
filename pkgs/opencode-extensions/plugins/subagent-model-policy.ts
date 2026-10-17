import { Plugin } from "@opencode/plugin";

const policy =
  "You may choose the model and reasoning variant for each delegated task. Honor explicit user choices. Before setting this parameter, use the models tool to look up available provider/model IDs and variants; never invent IDs. Prefer the invoking provider and lightweight models such as Luna for routine work; choose stronger models when the task warrants them. Use Astra only when the user explicitly requests it. Format the value as providerID/modelID or providerID/modelID#variant. Omit this parameter to use the agent-configured or inherited model.";

export default Plugin.define({
  id: "local.subagent-model-policy",
  async setup(ctx) {
    for (const hook of ["context", "compaction", "generate"] as const) {
      await ctx.session.hook(hook, (event) => {
        const tool = event.tools.subagent;
        if (!tool) return;
        const properties = tool.input.properties as
          | Record<string, { type?: unknown; description?: string }>
          | undefined;
        const model = properties?.model;
        if (!model || typeof model !== "object" || model.type !== "string")
          throw new Error(
            "Subagent model policy requires the native string model parameter",
          );
        model.description = policy;
      });
    }
  },
});
