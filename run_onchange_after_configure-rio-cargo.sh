#!/bin/sh
set -eu

command -v kwriteconfig6 >/dev/null 2>&1 || exit 0

rio="$HOME/.local/share/cargo/bin/rio"

# Note: terminal shortcuts (Ctrl+Alt+T etc.) are managed by
# run_onchange_after_configure-kde-shortcuts.sh; this script only sets the
# default-terminal association.
kwriteconfig6 --file kdeglobals --group General --key TerminalApplication --notify "$rio"
kwriteconfig6 --file kdeglobals --group General --key TerminalService --notify rio-cargo.desktop
