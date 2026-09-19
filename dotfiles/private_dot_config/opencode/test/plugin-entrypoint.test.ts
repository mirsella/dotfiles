import { describe, expect, test } from "bun:test";

// OpenCode's legacy plugin loader iterates every export of a plugin module
// and disables the whole plugin with "Plugin export is not a function" when
// any export is not a plugin function. The entrypoint must therefore expose
// exactly one runtime export: the default plugin factory.
describe("plugin entrypoint", () => {
	for (const name of ["subagent-watchdog", "doom-loop-threshold"]) {
		test(`${name} exposes exactly one function export`, async () => {
			const module = await import(`../plugins/${name}`);
			expect(Object.keys(module)).toEqual(["default"]);
			expect(typeof module.default).toBe("function");
		});
	}
});
