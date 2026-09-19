#!/bin/sh
set -eu

xkb_layout=us
xkb_model=pc105
xkb_variant=colemak_dh_iso
xkb_options=terminate:ctrl_alt_bksp
tty_keymap=mod-dh-iso-us

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

if [ "$(id -u)" -eq 0 ]; then
  die 'run this as your regular user; the script uses sudo only for localectl'
fi

command -v localectl >/dev/null 2>&1 || die 'localectl not found; cannot set system keyboard layouts'
command -v kwriteconfig6 >/dev/null 2>&1 || die 'kwriteconfig6 not found; cannot update KDE keyboard config'
command -v sudo >/dev/null 2>&1 || die 'sudo not found; cannot update system keyboard layouts'

if ! localectl list-x11-keymap-variants "$xkb_layout" | grep -Fx "$xkb_variant" >/dev/null 2>&1; then
  die "XKB variant $xkb_layout($xkb_variant) is not installed; install/update xkeyboard-config"
fi

if ! localectl list-keymaps | grep -Fx "$tty_keymap" >/dev/null 2>&1; then
  die "TTY keymap $tty_keymap is not installed; install/update kbd"
fi

kwriteconfig6 --file kxkbrc --group Layout --key Use true
kwriteconfig6 --file kxkbrc --group Layout --key LayoutList "$xkb_layout,$xkb_layout"
kwriteconfig6 --file kxkbrc --group Layout --key VariantList "$xkb_variant,"
kwriteconfig6 --file kxkbrc --group Layout --key DisplayNames 'col,'

sudo localectl set-x11-keymap "$xkb_layout" "$xkb_model" "$xkb_variant" "$xkb_options"
sudo localectl --no-convert set-keymap "$tty_keymap"

if command -v qdbus6 >/dev/null 2>&1; then
  qdbus6 org.kde.keyboard /Layouts setLayout 0 >/dev/null 2>&1 \
    || warn 'KDE keyboard service did not switch to layout 0; log out/in if the current session is not updated'
else
  warn 'qdbus6 not found; log out/in if the current KDE session is not updated'
fi

printf 'Applied ISO Colemak-DH keyboard settings.\n'
printf 'KDE/XKB: %s(%s)\n' "$xkb_layout" "$xkb_variant"
printf 'TTY: %s\n' "$tty_keymap"
