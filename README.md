# dotfiles

Personal dotfiles managed with [chezmoi](https://www.chezmoi.io/).

This repo contains shell, terminal, tmux, Neovim, SSH, desktop, and OpenCode configuration used on my machines.

## How It Is Organized

The layout follows chezmoi conventions, so files such as `private_dot_config/...` and executable templates map directly to the right place in `$HOME`.

The setup is also host-aware. Some shell files load extra configuration from machine-specific paths, which lets me keep one repo for multiple devices without flattening everything into a single generic config.

Notable pieces in this repo:

- Zsh setup with aliases, helper functions, and host-specific sourcing.
- Neovim bootstrap based on LazyVim plus custom plugins.
- WezTerm and tmux configuration for the terminal workflow.
- OpenCode configuration, plugins, and MCP integrations.
- Host-specific system backups under `system/<hostname>/`, intentionally
  excluded from `chezmoi apply`.

## Apply on a machine

```bash
chezmoi init --apply mirsella/dotfiles
```

## If the repo is already cloned

```bash
chezmoi apply
```

## Keyboard Layout

Apply ISO Colemak-DH for KDE, lock/login screens, and TTYs from the chezmoi source repo:

```bash
./apply-colemak-dhm-keyboard.sh
```

The script is ignored by `chezmoi apply`, so it stays repo-only and is not installed into `PATH`. It writes KDE's `kxkbrc`, sets the system XKB default with `localectl set-x11-keymap`, and sets the virtual-console keymap with `localectl --no-convert set-keymap`.

## System Backup

Refresh the current host's root configuration snapshot:

```bash
./update-system-backup.sh
```

The script stores the snapshot under `system/$(hostname -s)/`. Pass a hostname
as its first argument to override the destination name. These files are for
manual recovery and are never installed by `chezmoi apply`.

## Notes

- This is a personal setup, so review the files before applying everything as-is.
- Some tools and aliases assume my own environment and installed CLI stack.
- A few defaults are intentionally opinionated, like shell aliases and desktop-specific tweaks.
