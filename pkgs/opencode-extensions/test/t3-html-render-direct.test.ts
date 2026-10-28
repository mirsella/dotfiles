import { expect, test } from "bun:test";
import plugin from "../plugins/t3-html-render-direct";

type ToolOptions = {
  namespace?: string;
  permission?: string;
  codemode?: boolean;
};

type TestTool = {
  id: string;
  name: string;
  description: string;
  input: unknown;
  output?: unknown;
  execute: () => Promise<{ output: unknown }>;
  options?: ToolOptions;
};

type Editor = {
  list: () => TestTool[];
  add: (next: Omit<TestTool, "id">) => void;
};

const noOutput = async () => ({ output: null });

function tool(
  id: string,
  name: string,
  namespace: string,
  execute = noOutput,
  codemode = true,
): TestTool {
  return {
    id,
    name,
    description: "",
    input: {},
    execute,
    options: { namespace, codemode },
  };
}

function effectiveId(tool: Omit<TestTool, "id">) {
  return tool.options?.namespace === undefined
    ? tool.name
    : `${tool.options.namespace.replaceAll(".", "_")}_${tool.name}`;
}

function editor(tools: TestTool[]): Editor {
  return {
    list: () => tools,
    add: (next) => {
      const id = effectiveId(next);
      const index = tools.findIndex((tool) => tool.id === id);
      const replacement = { ...next, id };
      if (index === -1) tools.push(replacement);
      else tools[index] = replacement;
    },
  };
}

async function apply(tools: TestTool[]) {
  let transform: ((tools: Editor) => void) | undefined;
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
}

test("re-registers the T3 html_render in its own namespace as native", async () => {
  const execute = async () => ({ output: "rendered" });
  const tools = [
    tool("t3-code-thread_html_render", "html_render", "t3-code-thread", execute),
    tool("t3-code-thread_html_preview", "html_preview", "t3-code-thread"),
    tool("t3-code-thread_other", "other", "t3-code-thread"),
    tool("other_html_render", "html_render", "other"),
  ];

  await apply(tools);

  expect(tools.map((candidate) => candidate.id)).toEqual([
    "t3-code-thread_html_render",
    "t3-code-thread_html_preview",
    "t3-code-thread_other",
    "other_html_render",
  ]);
  const native = tools.find((candidate) => candidate.id === "t3-code-thread_html_render");
  expect(native?.options?.codemode).toBe(false);
  expect(native?.options?.namespace).toBe("t3-code-thread");
  expect(native?.execute).toBe(execute);
  expect(tools.some((candidate) => candidate.id === "html_render")).toBe(false);
});

test("keeps each thread's html_render independent", async () => {
  const first = async () => ({ output: "first" });
  const second = async () => ({ output: "second" });
  const tools = [
    tool("t3-code-a_html_render", "html_render", "t3-code-a", first),
    tool("t3-code-b_html_render", "html_render", "t3-code-b", second),
  ];

  await apply(tools);

  const [a, b] = tools;
  expect(a?.id).toBe("t3-code-a_html_render");
  expect(a?.options?.codemode).toBe(false);
  expect(a?.execute).toBe(first);
  expect(b?.id).toBe("t3-code-b_html_render");
  expect(b?.options?.codemode).toBe(false);
  expect(b?.execute).toBe(second);
  expect(tools.some((candidate) => candidate.id === "html_render")).toBe(false);
});

test("leaves non-T3 and already-native tools alone", async () => {
  const tools = [
    tool("t3-code-thread_html_render", "html_render", "t3-code-thread", noOutput, false),
    tool("other_html_render", "html_render", "other"),
  ];

  await apply(tools);

  expect(tools[0]?.options?.codemode).toBe(false);
  expect(tools[1]?.options?.codemode).toBe(true);
  expect(tools.some((candidate) => candidate.id === "html_render")).toBe(false);
});
