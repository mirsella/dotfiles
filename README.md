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

## Apply on a machine

```bash
chezmoi init --apply mirsella/dotfiles
```

## If the repo is already cloned

```bash
chezmoi apply
```

## Notes

- This is a personal setup, so review the files before applying everything as-is.
- Some tools and aliases assume my own environment and installed CLI stack.
- A few defaults are intentionally opinionated, like shell aliases and desktop-specific tweaks.
