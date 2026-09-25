#!/usr/bin/env sh
# Rebuild tui-plugins/cache-timer-tui.js from tui-plugins/cache-timer.tsx.
# Usage: sh tui-plugins/build-cache-timer.sh   (run from anywhere)
#
# Toolchain pins matter. babel-preset-solid must stay on 1.8.x and be
# compiled with generate=universal + moduleName=@opentui/solid. Newer
# preset defaults emit solid-js/web imports, which resolve nowhere in the
# TUI, so the plugin fails silently (no toast, no timer, no log).
set -eu
SRC_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORK=/tmp/cache-timer-build
rm -rf "$WORK"
mkdir -p "$WORK"
cat > "$WORK/package.json" <<'JSON'
{
  "private": true,
  "type": "commonjs",
  "dependencies": {
    "@babel/core": "^7.24.0",
    "@babel/preset-typescript": "^7.24.0",
    "babel-preset-solid": "~1.8.0"
  }
}
JSON
cat > "$WORK/compile.js" <<'JS'
const babel = require("@babel/core");
const { writeFileSync } = require("node:fs");
const { join } = require("node:path");
const src = join(process.env.CT_SRC_DIR, "cache-timer.tsx");
const out = babel.transformFileSync(src, {
  presets: [
    ["babel-preset-solid", { generate: "universal", moduleName: "@opentui/solid" }],
    "@babel/preset-typescript",
  ],
  filename: "cache-timer.tsx",
});
writeFileSync(join(process.env.CT_SRC_DIR, "cache-timer-tui.js"), out.code);
console.log("built ok,", out.code.length, "bytes");
JS
cd "$WORK"
bun install --silent
CT_SRC_DIR="$SRC_DIR" node compile.js
rm -rf "$WORK"
