import { expect, test } from "bun:test";
for (const name of [
  "composio-auth",
  "git-no-ext-diff",
  "subagent-model-policy",
  "t3-html-render-direct",
]) {
  test(`${name} has a native v2 definition`, async () => {
    const module = await import(`../plugins/${name}`);
    expect(Object.keys(module)).toEqual(["default"]);
    expect(typeof module.default.id).toBe("string");
    expect(typeof module.default.setup).toBe("function");
  });
}
