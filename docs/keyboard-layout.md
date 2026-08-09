# Keyboard Layout

Apply ISO Colemak-DH for KDE, lock/login screens, and TTYs from the chezmoi
source repo:

```bash
./apply-colemak-dhm-keyboard.sh
```

The script is ignored by `chezmoi apply`, so it stays repo-only and is not
installed into `PATH`. It writes KDE's `kxkbrc`, sets the system XKB default
with `localectl set-x11-keymap`, and sets the virtual-console keymap with
`localectl --no-convert set-keymap`.
