#!/bin/sh
set -eu

repo="$HOME/.cache/opencode/tui-plugins/opencode-dcp"
marker="$repo/node_modules/.chezmoi-dcp-commit"

if [ ! -d "$repo/.git" ]; then
    echo "OpenCode DCP external is missing at $repo" >&2
    exit 1
fi

commit=$(git -C "$repo" rev-parse HEAD)
installed_commit=
if [ -r "$marker" ]; then
    IFS= read -r installed_commit < "$marker" || true
fi

if [ "$installed_commit" = "$commit" ]; then
    exit 0
fi

if ! command -v npm >/dev/null 2>&1; then
    echo "npm is required to install OpenCode DCP TUI dependencies" >&2
    exit 1
fi

npm ci --prefix "$repo"
printf '%s\n' "$commit" > "$marker"
