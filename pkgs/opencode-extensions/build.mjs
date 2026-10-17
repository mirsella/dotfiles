import { transformFileSync } from "@babel/core";
import { readdirSync, mkdirSync, writeFileSync, rmSync } from "node:fs";

// Retired plugins must not survive in a reused local build directory.
rmSync(new URL("./dist/", import.meta.url), { recursive: true, force: true });

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
  external: ["@opencode/plugin/tui", "@opentui/core", "@opentui/solid", "solid-js"],
  plugins: [
    {
      name: "solid-universal",
      setup(build) {
        // OpenTUI needs Solid's universal renderer rather than Bun's JSX transform.
        build.onLoad({ filter: /\.tsx$/ }, ({ path }) => {
          const result = transformFileSync(path, {
            presets: [
              [
                "babel-preset-solid",
                { generate: "universal", moduleName: "@opentui/solid" },
              ],
              "@babel/preset-typescript",
            ],
          });
          if (!result?.code)
            throw new Error(`TSX compilation produced no code: ${path}`);
          return { contents: result.code, loader: "js" };
        });
      },
    },
  ],
});
if (!result.success)
  throw new AggregateError(result.logs, "Plugin bundling failed");

// v2 resolves a local TUI package through its physical tui entrypoint.
for (const file of readdirSync("dist/tui-plugins").filter((file) =>
  file.endsWith(".js"),
)) {
  const name = file.replace(/(?:-tui)?\.js$/, "");
  mkdirSync(`dist/tui-plugins/${name}`, { recursive: true });
  writeFileSync(
    `dist/tui-plugins/${name}/tui.js`,
    `export { default } from "../${file}";\n`,
  );
}
