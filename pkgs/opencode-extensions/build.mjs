import { rmSync } from "node:fs";

// Retired plugins must not survive in a reused local build directory.
rmSync(new URL("./dist/", import.meta.url), { recursive: true, force: true });

const result = await Bun.build({
  entrypoints: [...new Bun.Glob("plugins/*.ts").scanSync(".")],
  root: ".",
  outdir: "dist",
  target: "bun",
  format: "esm",
  splitting: true,
  naming: { chunk: "chunks/[name]-[hash].[ext]" },
});
if (!result.success)
  throw new AggregateError(result.logs, "Plugin bundling failed");
