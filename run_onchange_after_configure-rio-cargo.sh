#!/bin/sh
set -eu

command -v kwriteconfig6 >/dev/null 2>&1 || exit 0

rio="$HOME/.local/share/cargo/bin/rio"

kwriteconfig6 --file kdeglobals --group General --key TerminalApplication --notify "$rio"
kwriteconfig6 --file kdeglobals --group General --key TerminalService --notify rio-cargo.desktop
kwriteconfig6 --file kglobalshortcutsrc --group services --group rio.desktop --key _launch --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.wezfurlong.wezterm.desktop --key _launch --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group rio-cargo.desktop --key _launch --notify Ctrl+Alt+T

if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 --noincremental
fi

runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
bus_address="unix:path=$runtime_dir/bus"
ctrl_alt_t=201326676

if command -v busctl >/dev/null 2>&1 \
    && [ -S "$runtime_dir/bus" ] \
    && busctl --address="$bus_address" status org.kde.kglobalaccel >/dev/null 2>&1
then
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel unregister ss rio.desktop _launch >/dev/null || true
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel setForeignShortcut asai 4 \
        org.wezfurlong.wezterm.desktop _launch WezTerm WezTerm 1 0 >/dev/null
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel doRegister as 4 \
        rio-cargo.desktop _launch "Rio (Cargo)" "Rio (Cargo)" >/dev/null
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel setForeignShortcut asai 4 \
        rio-cargo.desktop _launch "Rio (Cargo)" "Rio (Cargo)" 1 "$ctrl_alt_t" >/dev/null
fi
