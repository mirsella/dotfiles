import { expect, test } from "bun:test";
import plugin from "../plugins/git-no-ext-diff";
test("native shell hook suppresses external diff without rewriting unrelated commands", async () => {
  let hook: (event: any) => unknown;
  await plugin.setup({
    tool: {
      hook: async (name: string, callback: typeof hook) => {
        expect(name).toBe("execute.before");
        hook = callback;
      },
    },
  } as any);
  const input = {
    command: "git diff; git -C repo show HEAD; printf 'git diff'",
  };
  await hook!({ tool: "shell", input });
  expect(input.command).toBe(
    "git diff --no-ext-diff; git -C repo show --no-ext-diff HEAD; printf 'git diff'",
  );
  const unchanged = { command: "git diff --no-ext-diff" };
  await hook!({ tool: "shell", input: unchanged });
  expect(unchanged.command).toBe("git diff --no-ext-diff");
  const other = { command: "git diff" };
  await hook!({ tool: "custom_shell", input: other });
  expect(other.command).toBe("git diff");
});
