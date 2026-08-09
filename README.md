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

The script stores the snapshot under `system/$(hostname -s)/`. These files are
for manual recovery and are never installed by `chezmoi apply`.

## Cargo OOM Protection

Chezmoi installs `~/.local/share/cargo/bin/cargo`, which launches each Cargo
command in a dedicated transient systemd scope with `oom_score_adj=500`. This
lets the kernel prefer Cargo and its compiler/linker children as OOM victims,
while systemd-oomd can kill the complete build scope without killing the shell
or terminal. No hard memory limit is imposed.

Enable the global swap safety net on a systemd-based Linux host:

```bash
sudo systemctl enable --now systemd-oomd.service
sudo systemctl set-property -- -.slice ManagedOOMSwap=kill
```

Verify the setup:

```bash
systemctl is-active systemd-oomd.service
systemctl show -p ManagedOOMSwap -- -.slice
oomctl --no-pager
```

The default systemd-oomd swap threshold is 90% of swap usage, not RAM usage.
Keep `~/.local/share/cargo/bin` before `/usr/bin` in `PATH`. Rustup toolchain
selection through `cargo +nightly`, `cargo +stable`, `rust-toolchain.toml`, and
directory overrides remains covered. `rustup run nightly cargo ...` bypasses
the wrapper and should be avoided when OOM protection is wanted.

To remove the global swap policy while retaining the kernel's normal OOM
killer and the Cargo score adjustment:

```bash
sudo systemctl set-property -- -.slice ManagedOOMSwap=auto
sudo systemctl disable --now systemd-oomd.service
```

## Main Hotspot

Restore the `main hotspot` NetworkManager profile and its narrowly scoped
Docker forwarding rules on host `main`:

```bash
./apply-main-hotspot.sh
```

The script retrieves the WPA2 PSK from the `main hotspot` Wi-Fi item in the
Proton Pass `Personal` vault. The password is not stored in this repository.
It installs the root-owned files from `system/main/`, restarts Docker, and
activates the system-wide, pre-login NetworkManager hotspot profile. It aborts
unless the computer's short hostname is exactly `main`; normal `chezmoi apply`
never installs these files.

## Notes

- This is a personal setup, so review the files before applying everything as-is.
- Some tools and aliases assume my own environment and installed CLI stack.
- A few defaults are intentionally opinionated, like shell aliases and desktop-specific tweaks.
