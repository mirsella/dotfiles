import { afterEach, expect, spyOn, test } from "bun:test";
import type { Config, PluginInput } from "@opencode-ai/plugin";
import localLlama from "../plugins/local-llama";

const fetchMock = spyOn(globalThis, "fetch");
afterEach(() => fetchMock.mockReset());

async function discover() {
	const logs: unknown[] = [];
	const client = { app: { log: async (entry: unknown) => { logs.push(entry); } } };
	const hooks = await localLlama({ client } as unknown as PluginInput);
	const config: Config = { provider: { cloud: { models: { remote: {} } } } };
	await hooks.config(config);
	return { config, logs };
}

test("discovers IDs and uses serving context, not the larger training context", async () => {
	fetchMock.mockResolvedValue(Response.json({ data: [
		{ id: "new-model", meta: { n_ctx: 32768, n_ctx_train: 262144 } },
		{ id: "small-context", meta: { n_ctx: 4096 } },
	] }));
	const { config, logs } = await discover();
	expect(config.provider?.["llama.cpp"]?.models?.["new-model"]?.limit).toEqual({ context: 32768, output: 8192 });
	expect(config.provider?.["llama.cpp"]?.models?.["small-context"]?.limit).toEqual({ context: 4096, output: 1024 });
	expect(config.provider?.cloud).toEqual({ models: { remote: {} } });
	expect(logs).toEqual([]);
	expect(fetchMock.mock.calls[0][0]).toBe("http://127.0.0.1:8080/v1/models");
	expect(fetchMock.mock.calls[0][1]?.signal).toBeInstanceOf(AbortSignal);
});

for (const [name, response] of [
	["loading", () => new Response("loading", { status: 503 })],
	["malformed list", () => Response.json({ models: [] })],
	["missing runtime context", () => Response.json({ data: [{ id: "model", meta: { n_ctx_train: 262144 } }] })],
] as const) {
	test(`${name} logs a diagnostic and preserves other providers`, async () => {
		fetchMock.mockResolvedValue(response());
		const { config, logs } = await discover();
		expect(config.provider).toEqual({ cloud: { models: { remote: {} } } });
		expect(logs).toHaveLength(1);
	});
}

test("a stopped server does not prevent cloud use", async () => {
	fetchMock.mockRejectedValue(new TypeError("Connection refused"));
	const { config, logs } = await discover();
	expect(config.provider).toEqual({ cloud: { models: { remote: {} } } });
	expect(logs).toHaveLength(1);
});

test("exposes only thinking as an enabled variant with parsed reasoning", async () => {
	fetchMock.mockResolvedValue(Response.json({ data: [{ id: "local", meta: { n_ctx: 32768 } }] }));
	const { config } = await discover();
	const model = config.provider?.["llama.cpp"]?.models?.local;
	expect(model?.reasoning).toBe(true);
	expect(model?.interleaved).toEqual({ field: "reasoning_content" });
	expect(model?.options).toEqual({ reasoning_format: "deepseek" });
	expect(model?.variants).toEqual({
		thinking: { chat_template_kwargs: { enable_thinking: true } },
		low: { disabled: true },
		medium: { disabled: true },
		high: { disabled: true },
	});
});
