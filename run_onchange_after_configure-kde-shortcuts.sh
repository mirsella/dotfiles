#!/bin/sh
# Applies the custom KDE global shortcuts tracked in chezmoi.
# Source of truth for all deviations from KDE defaults in kglobalshortcutsrc:
# custom KWin quick tiles, freed Meta+Ctrl+ arrows, stripped Meta aliases,
# Plasma activity/clipboard shortcuts, Spectacle, Yakuake, and custom launchers
# (clear-notifications Meta+S, Rio Cargo Ctrl+Alt+T, disabled stock terminals).
# Idempotent: re-running writes the same values. kglobalaccel is notified live
# via kwriteconfig6 --notify plus busctl; some changes may need re-login.
set -eu

command -v kwriteconfig6 >/dev/null 2>&1 || exit 0

# --- KWin: custom quick tiles (repurposed Meta+Ctrl+arrows) ---
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Window Custom Quick Tile Top' --notify 'Meta+Ctrl+Up,none,Custom Quick Tile Window to the Top'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Window Custom Quick Tile Left' --notify 'Meta+Ctrl+Left,none,Custom Quick Tile Window to the Left'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Window Custom Quick Tile Right' --notify 'Meta+Ctrl+Right,none,Custom Quick Tile Window to the Right'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Window Custom Quick Tile Bottom' --notify 'Meta+Ctrl+PgDown,none,Custom Quick Tile Window to the Bottom'

# --- KWin: free the arrows for the tiles above (keep Down on default) ---
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch One Desktop Up' --notify 'none,Meta+Ctrl+Up,Switch One Desktop Up'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch One Desktop to the Left' --notify 'none,Meta+Ctrl+Left,Switch One Desktop to the Left'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch One Desktop to the Right' --notify 'none,Meta+Ctrl+Right,Switch One Desktop to the Right'

# --- KWin: strip Meta fallbacks (keep primary only) ---
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Expose' --notify 'Ctrl+F9,Ctrl+F9\tMeta+F9,Toggle Present Windows (Current desktop)'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'ExposeAll' --notify 'Launch (C)\tCtrl+F10,Launch (C)\tCtrl+F10\tMeta+F10,Toggle Present Windows (All desktops)'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'ExposeClass' --notify 'Ctrl+F7,Ctrl+F7\tMeta+F7,Toggle Present Windows (Window class)'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch to Desktop 1' --notify 'Ctrl+F1,Ctrl+F1\tMeta+F1,Switch to Desktop 1'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch to Desktop 2' --notify 'Ctrl+F2,Ctrl+F2\tMeta+F2,Switch to Desktop 2'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch to Desktop 3' --notify 'Ctrl+F3,Ctrl+F3\tMeta+F3,Switch to Desktop 3'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Switch to Desktop 4' --notify 'Ctrl+F4,Ctrl+F4\tMeta+F4,Switch to Desktop 4'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Walk Through Windows' --notify 'Alt+Tab,Alt+Tab\tMeta+Tab,Walk Through Windows'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Walk Through Windows (Reverse)' --notify 'Alt+Shift+Tab,Alt+Shift+Tab\tMeta+Shift+Tab,Walk Through Windows (Reverse)'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Walk Through Windows of Current Application' --notify 'Alt+`,Alt+`\tMeta+`,Walk Through Windows of Current Application'
kwriteconfig6 --file kglobalshortcutsrc --group kwin --key 'Walk Through Windows of Current Application (Reverse)' --notify 'Alt+~,Alt+~\tMeta+~,Walk Through Windows of Current Application (Reverse)'

# --- Plasma shell: activities + clipboard ---
kwriteconfig6 --file kglobalshortcutsrc --group plasmashell --key 'next activity' --notify 'Meta+A,none,Walk through activities'
kwriteconfig6 --file kglobalshortcutsrc --group plasmashell --key 'previous activity' --notify 'Meta+Shift+A,none,Walk through activities (Reverse)'
kwriteconfig6 --file kglobalshortcutsrc --group plasmashell --key 'repeat_action' --notify 'Meta+Ctrl+R,none,Manually Invoke Action on Current Clipboard'

# --- Yakuake: dropdown disabled ---
kwriteconfig6 --file kglobalshortcutsrc --group yakuake --key 'toggle-window-state' --notify 'none,F12,Open/Retract Yakuake'

# --- Spectacle: current-monitor on Shift+Print, rest disabled ---
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.kde.spectacle.desktop --key CurrentMonitorScreenShot --notify Shift+Print
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.kde.spectacle.desktop --key FullScreenScreenShot --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.kde.spectacle.desktop --key OpenWithoutScreenshot --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.kde.spectacle.desktop --key RecordWindow --notify none

# --- Custom launchers ---
kwriteconfig6 --file kglobalshortcutsrc --group services --group net.local.bash.desktop --key _launch --notify Meta+S
kwriteconfig6 --file kglobalshortcutsrc --group services --group rio-cargo.desktop --key _launch --notify Ctrl+Alt+T
kwriteconfig6 --file kglobalshortcutsrc --group services --group rio-cargo.desktop --key New --notify Ctrl+Alt+T

# --- Disable stock terminal launchers (Rio Cargo owns Ctrl+Alt+T) ---
kwriteconfig6 --file kglobalshortcutsrc --group services --group com.mitchellh.ghostty.desktop --key _launch --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.kde.konsole.desktop --key _launch --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group org.wezfurlong.wezterm.desktop --key _launch --notify none
kwriteconfig6 --file kglobalshortcutsrc --group services --group rio.desktop --key _launch --notify none

if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 --noincremental
fi

# Live-reload service shortcuts in a running session (file writes alone may
# not take effect until re-login). Key codes: Ctrl+Alt+T = 201326676,
# Meta+S = 268435539.
runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
bus_address="unix:path=$runtime_dir/bus"
ctrl_alt_t=201326676
meta_s=268435539

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
        rio-cargo.desktop _launch 'Rio (Cargo)' 'Rio (Cargo)' >/dev/null
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel setForeignShortcut asai 4 \
        rio-cargo.desktop _launch 'Rio (Cargo)' 'Rio (Cargo)' 1 "$ctrl_alt_t" >/dev/null
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel doRegister as 4 \
        net.local.bash.desktop _launch 'clear notifications' 'clear notifications' >/dev/null
    busctl --address="$bus_address" call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel setForeignShortcut asai 4 \
        net.local.bash.desktop _launch 'clear notifications' 'clear notifications' 1 "$meta_s" >/dev/null
fi

if command -v qdbus6 >/dev/null 2>&1; then
    qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure >/dev/null 2>&1 || true
fi
