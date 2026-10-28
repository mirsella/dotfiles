import { expect, test } from "bun:test";
import plugin from "../plugins/t3-html-render-direct";

type TestTool = {
  id: string;
  name: string;
  description: string;
  input: unknown;
  output?: unknown;
  execute: () => Promise<{ output: unknown }>;
  options?: { namespace?: string; codemode?: boolean };
};

const execute = async () => ({ output: null });

function tool(id: string, name: string, namespace: string): TestTool {
  return {
    id,
    name,
    description: "",
    input: {},
    execute,
    options: { namespace, codemode: true },
  };
}

function editor(tools: TestTool[]) {
  return {
    list: () => tools,
    get: (id: string) => tools.find((tool) => tool.id === id),
    add: (next: Omit<TestTool, "id">) => {
      tools.push({ ...next, id: next.name });
    },
    remove: (id: string) => {
      const index = tools.findIndex((tool) => tool.id === id);
      if (index !== -1) tools.splice(index, 1);
    },
  };
}

test("replaces the namespaced T3 html_render with a direct html_render", async () => {
  const tools = [
    tool("t3-code-thread_html_render", "html_render", "t3-code-thread"),
    tool("t3-code-thread_html_preview", "html_preview", "t3-code-thread"),
    tool("t3-code-thread_other", "other", "t3-code-thread"),
    tool("other_html_render", "html_render", "other"),
  ];

  let transform: ((tools: ReturnType<typeof editor>) => void) | undefined;
  const ctx = {
    tool: {
      transform: async (callback: typeof transform) => {
        transform = callback;
        return { dispose: async () => {} };
      },
    },
  };

  await plugin.setup(ctx as never);
  transform!(editor(tools));

  expect(tools.map((candidate) => candidate.id)).toEqual([
    "t3-code-thread_html_preview",
    "t3-code-thread_other",
    "other_html_render",
    "html_render",
  ]);
  const alias = tools.find((candidate) => candidate.id === "html_render");
  expect(alias?.options?.codemode).toBe(false);
  expect(alias?.execute).toBe(execute);
});

test("does not add a second alias when the namespaced tool comes back", async () => {
  const tools = [
    tool("t3-code-thread_html_render", "html_render", "t3-code-thread"),
    tool("html_render", "html_render", ""),
  ];
  const next = editor(tools);
  let transform: ((tools: ReturnType<typeof editor>) => void) | undefined;
  const ctx = {
    tool: {
      transform: async (callback: typeof transform) => {
        transform = callback;
        return { dispose: async () => {} };
      },
    },
  };

  await plugin.setup(ctx as never);
  transform!(next);

  expect(tools.filter((candidate) => candidate.id === "html_render")).toHaveLength(1);
  expect(tools.some((candidate) => candidate.id === "t3-code-thread_html_render")).toBe(false);
});
