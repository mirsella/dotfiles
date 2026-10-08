import { transformFileSync } from "@babel/core";
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";

const result = await Bun.build({
  entrypoints: ["plugins", "tui-plugins"].flatMap((directory) =>
    readdirSync(directory)
      .filter((name) => name.endsWith(".ts"))
      .map((name) => `${directory}/${name}`),
  ),
  root: ".",
  outdir: "dist",
  target: "bun",
  format: "esm",
  external: ["@opentui/solid", "solid-js"],
});
if (!result.success) throw new AggregateError(result.logs, "Plugin bundling failed");

// OpenTUI needs Solid's universal renderer rather than Bun's JSX transform.
const timer = transformFileSync("tui-plugins/cache-timer.tsx", {
  presets: [
    ["babel-preset-solid", { generate: "universal", moduleName: "@opentui/solid" }],
    "@babel/preset-typescript",
  ],
});
if (!timer?.code) throw new Error("Cache timer compilation produced no code");
mkdirSync("dist/tui-plugins", { recursive: true });
writeFileSync("dist/tui-plugins/cache-timer-tui.js", timer.code);
