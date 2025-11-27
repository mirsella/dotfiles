import { type Plugin, tool } from "@opencode-ai/plugin";
import { spawn, type Subprocess } from "bun";

const OSGREP_INSTRUCTIONS = `
## osgrep - Semantic Code Search

osgrep is a semantic search tool for codebases. Prefer osgrep over grep, find, or glob when searching for concepts, behaviors, or logic.

### When to use osgrep
- Locating where a feature is implemented
- Understanding how a behavior works conceptually
- Finding files or symbols related to a concept
- Exploring unfamiliar codebases

### Decision Policy
- Use osgrep for concept, behavior, or logic queries
- Use literal matching tools (grep/glob) only when an exact identifier, string, or regex is needed and osgrep fails

### Search Strategy
1. Run osgrep with your query
2. Note the file paths and line numbers from results
3. If the snippet is sufficient, you're done
4. If more context is needed, use Read tool to read the file around the specific lines
5. If results are vague, rerun with a more specific query or higher limit

### Available Options
- limit: Max total results (default: 25), use 50 for broad surveys
- perFile: Max matches per file (default: 1), use 5 for implementation details
- path: Search within a specific subdirectory
- sync: Force re-index before searching if index may be stale
`;

let serveProcess: Subprocess | null = null;

export const OsgrepPlugin: Plugin = async ({ $, directory }) => {
	serveProcess = spawn(["osgrep", "serve"], {
		cwd: directory,
		stdout: "ignore",
		stderr: "ignore",
	});

	process.on("exit", () => serveProcess?.kill());
	process.on("SIGINT", () => serveProcess?.kill());
	process.on("SIGTERM", () => serveProcess?.kill());

	return {
		instructions: OSGREP_INSTRUCTIONS,
		tool: {
			osgrep: tool({
				description:
					"Semantic code search using natural language. Finds concepts, behaviors, and logic in the codebase. Prefer this over grep/find for conceptual queries.",
				args: {
					query: tool.schema
						.string()
						.describe(
							'Natural language query describing what to find (e.g. "How are user authentication tokens validated?")',
						),
					path: tool.schema
						.string()
						.optional()
						.describe("Subdirectory to search within"),
					limit: tool.schema
						.number()
						.optional()
						.describe("Max total results to return (default: 25)"),
					perFile: tool.schema
						.number()
						.optional()
						.describe("Max matches per file (default: 1)"),
					sync: tool.schema
						.boolean()
						.optional()
						.describe("Force re-index before searching"),
				},
				async execute(args) {
					const cmdParts = ["osgrep", "--plain"];

					if (args.limit) {
						cmdParts.push("-m", String(args.limit));
					}
					if (args.perFile) {
						cmdParts.push("--per-file", String(args.perFile));
					}
					if (args.sync) {
						cmdParts.push("--sync");
					}

					cmdParts.push(JSON.stringify(args.query));

					if (args.path) {
						cmdParts.push(args.path);
					}

					const cmd = cmdParts.join(" ");
					const result = await $`sh -c ${cmd}`.quiet().nothrow();

					if (result.exitCode !== 0) {
						return `osgrep failed (exit ${result.exitCode}): ${result.stderr.toString()}`;
					}

					return result.stdout.toString();
				},
			}),
		},
	};
};
