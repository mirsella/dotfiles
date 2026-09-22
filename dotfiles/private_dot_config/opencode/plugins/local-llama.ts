import type { Config, Plugin } from "@opencode-ai/plugin";

export default (async ({ client }) => ({
	async config(config) {
		const baseURL = "http://127.0.0.1:8080/v1";
		try {
			const response = await fetch(`${baseURL}/models`, {
				signal: AbortSignal.timeout(2000),
			});
			if (!response.ok) throw new Error(`llama-server returned HTTP ${response.status}`);
			const body = await response.json();
			if (!Array.isArray(body.data)) throw new Error("Missing llama-server model list");
			const models: NonNullable<NonNullable<Config["provider"]>[string]["models"]> = {};
			for (const model of body.data) {
				const context = model.meta?.n_ctx;
				if (typeof model.id !== "string" || !model.id || !Number.isSafeInteger(context) || context < 4) {
					throw new Error("llama-server model is missing its ID or runtime context size");
				}
				models[model.id] = {
					name: model.id,
					tool_call: true,
					reasoning: true,
					interleaved: { field: "reasoning_content" },
					options: { reasoning_format: "deepseek" },
					variants: {
						thinking: { chat_template_kwargs: { enable_thinking: true } },
						// Qwen uses a thinking toggle, not OpenAI's effort levels.
						low: { disabled: true },
						medium: { disabled: true },
						high: { disabled: true },
					},
					limit: { context, output: Math.min(8192, Math.floor(context / 4)) },
					modalities: { input: ["text"], output: ["text"] },
				};
			}
			config.provider ??= {};
			config.provider["llama.cpp"] = {
				npm: "@ai-sdk/openai-compatible",
				name: "Local llama.cpp",
				options: { baseURL },
				models,
			};
		} catch (error) {
			// The local server is optional; cloud providers must still start when it is stopped.
			await client.app.log({ body: {
				service: "local-llama",
				level: "warn",
				message: `Local model discovery failed: ${String(error)}. Start llama-server, then restart OpenCode to retry.`,
			} });
		}
	},
})) satisfies Plugin;
