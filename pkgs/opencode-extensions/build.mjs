import { transformFileSync } from "@babel/core";
import { readdirSync } from "node:fs";

const result = await Bun.build({
  entrypoints: ["plugins", "tui-plugins"].flatMap((directory) =>
    readdirSync(directory)
      .filter((name) => /\.tsx?$/.test(name))
      .map((name) => `${directory}/${name}`),
  ),
  root: ".",
  outdir: "dist",
  target: "bun",
  format: "esm",
  splitting: true,
  naming: { chunk: "chunks/[name]-[hash].[ext]" },
  external: ["@opentui/solid", "solid-js"],
  plugins: [{
    name: "solid-universal",
    setup(build) {
      // OpenTUI needs Solid's universal renderer rather than Bun's JSX transform.
      build.onLoad({ filter: /\.tsx$/ }, ({ path }) => {
        const result = transformFileSync(path, {
          presets: [
            ["babel-preset-solid", { generate: "universal", moduleName: "@opentui/solid" }],
            "@babel/preset-typescript",
          ],
        });
        if (!result?.code) throw new Error(`TSX compilation produced no code: ${path}`);
        return { contents: result.code, loader: "js" };
      });
    },
  }],
});
if (!result.success) throw new AggregateError(result.logs, "Plugin bundling failed");
