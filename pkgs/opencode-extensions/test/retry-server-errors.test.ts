import { expect, mock, test } from "bun:test";
import type { AssistantMessage } from "@opencode-ai/sdk";
import { RetryServerErrorsPlugin } from "../plugins/retry-server-errors";

const createMockClient = () => {
	const messagesMock = mock(async () => ({ data: [] }));
	const messageMock = mock(async () => ({ data: { info: { role: "user" } } }));
	const promptAsyncMock = mock(async () => ({}));
	const logMock = mock(async () => ({}));
	const showToastMock = mock(async () => ({}));

	return {
		client: {
			session: {
				messages: messagesMock,
				message: messageMock,
				promptAsync: promptAsyncMock,
			},
			app: {
				log: logMock,
			},
			tui: {
				showToast: showToastMock,
			},
		} as any,
		mocks: {
			messagesMock,
			messageMock,
			promptAsyncMock,
			logMock,
			showToastMock,
		},
	};
};

const assistantError = (error: AssistantMessage["error"]): AssistantMessage => ({
	id: "msg_err",
	sessionID: "ses_1",
	role: "assistant",
	time: { created: 100 },
	parentID: "msg_user1",
	modelID: "gpt-6-astra",
	providerID: "openai",
	mode: "build",
	path: { cwd: "/tmp", root: "/tmp" },
	cost: 0,
	tokens: { input: 0, output: 0, reasoning: 0, cache: { read: 0, write: 0 } },
	error,
});

test("does not retry when APIError message indicates usage limit reached", async () => {
	const { client, mocks } = createMockClient();
	const plugin = await RetryServerErrorsPlugin({
		client,
		directory: "/tmp",
		serverUrl: new URL("http://127.0.0.1:4096"),
	});

	await plugin.event?.({
		event: {
			type: "message.updated",
			properties: {
				info: assistantError({
					name: "APIError",
					data: {
						message: "The usage limit has been reached",
						isRetryable: true,
					},
				}),
			},
		} as any,
	});

	expect(mocks.messagesMock).not.toHaveBeenCalled();
	expect(mocks.promptAsyncMock).not.toHaveBeenCalled();
});

test("does not retry when APIError responseBody indicates usage limit reached", async () => {
	const { client, mocks } = createMockClient();
	const plugin = await RetryServerErrorsPlugin({
		client,
		directory: "/tmp",
		serverUrl: new URL("http://127.0.0.1:4096"),
	});

	await plugin.event?.({
		event: {
			type: "message.updated",
			properties: {
				info: assistantError({
					name: "APIError",
					data: {
						message: "Rate limit exceeded",
						responseBody: '{"error": "The usage limit has been reached"}',
						isRetryable: true,
					},
				}),
			},
		} as any,
	});

	expect(mocks.messagesMock).not.toHaveBeenCalled();
	expect(mocks.promptAsyncMock).not.toHaveBeenCalled();
});

test("does not retry when APIError is not retryable", async () => {
	const { client, mocks } = createMockClient();
	const plugin = await RetryServerErrorsPlugin({
		client,
		directory: "/tmp",
		serverUrl: new URL("http://127.0.0.1:4096"),
	});

	await plugin.event?.({
		event: {
			type: "message.updated",
			properties: {
				info: assistantError({
					name: "APIError",
					data: {
						message: "Invalid request parameter",
						isRetryable: false,
					},
				}),
			},
		} as any,
	});

	expect(mocks.messagesMock).not.toHaveBeenCalled();
	expect(mocks.promptAsyncMock).not.toHaveBeenCalled();
});

test("retries retryable APIError without non-retryable pattern", async () => {
	const { client, mocks } = createMockClient();
	mocks.messagesMock.mockImplementation(async () => ({
		data: [
			{
				info: {
					id: "msg_user1",
					sessionID: "ses_1",
					role: "user",
					time: { created: 50 },
					agent: "build",
					model: { providerID: "openai", modelID: "gpt-6-astra" },
				},
				parts: [{ type: "text", text: "hello" }],
			},
			{
				info: {
					id: "msg_err",
					sessionID: "ses_1",
					role: "assistant",
					parentID: "msg_user1",
					time: { created: 100 },
					modelID: "gpt-6-astra",
					providerID: "openai",
				},
				parts: [],
			},
		],
	}));

	const plugin = await RetryServerErrorsPlugin({
		client,
		directory: "/tmp",
		serverUrl: new URL("http://127.0.0.1:4096"),
	});

	await plugin.event?.({
		event: {
			type: "message.updated",
			properties: {
				info: assistantError({
					name: "APIError",
					data: {
						message: "Upstream request failed: [service_overloaded] Please retry.",
						isRetryable: true,
					},
				}),
			},
		} as any,
	});

	expect(mocks.messagesMock).toHaveBeenCalled();
});
