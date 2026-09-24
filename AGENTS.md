# Repository workflow

The checkout is `~/dev/dotfiles` on every machine. `.chezmoiroot` selects
`dotfiles/` as the chezmoi source; the flake at the repo root defines NixOS
`predator` and standalone Home Manager homes for Arch `laptop` and `main`.

- Chezmoi owns editable app configuration. Home Manager owns packages, generated
  Git/SSH settings, user services and SOPS wiring. Keep each path under one
  owner; `.chezmoiignore` lists the HM-owned paths.
- Chezmoi auto-commits and auto-pushes source edits. Its Git operation can include
  unrelated changes in this shared repository, including staged Nix changes.
  Review or finish those changes before `chezmoi add`, `edit` or `re-add`.
- On Arch, pacman/AUR own application binaries; the HM profiles install config
  and services. On Predator, only rebuild the NixOS target, never activate a
  standalone HM profile for the same user.
- Before deploying to Predator, inspect its `~/dev/dotfiles` status and diff,
  especially `flake.lock` (the weekly upgrade updates nixpkgs there). Sync only
  reviewed changes; do not use whole-tree `rsync --delete` over concurrent work.
  Rebuild with `ssh predator 'sudo nixos-rebuild switch --flake path:/home/mirsella/dev/dotfiles#predator'`.
  Run long rebuilds detached and poll the unit log and exit status.
- `nix flake check --no-build --no-update-lock-file` evaluates the flake;
  `nix eval --impure --json --file tests/invariants.nix` checks boot and disk invariants.
  Run the relevant Python tests for changed monitoring, unlock or suspend code.
- `disko.nix` formats **all three data disks** only for a fresh, intentional
  installation. To reinstall on existing disks, mount them without running disko,
  preserve the LUKS headers, ZFS pools, Secure Boot signing bundle (`/var/lib/sbctl`),
  HDD keys (`/etc/luks/`), and SOPS host identity (`/etc/ssh/ssh_host_ed25519_key`),
  then install `#predator`.
  The database dumps and ZFS snapshots on `tank/backup` are on-machine only.

Manual files outside the repo:
- Laptop uses Plasma lid defaults; Predator ignores lid-close via
  `configuration.nix` (screen off instead of sleep).
- Arch boxes: Nix daemon cache (`/etc/nix/nix.conf`), system Caddy
  (`/etc/caddy/Caddyfile`, `caddy.service`), udev rules and pacman hooks. The
  daemon ignores cache settings from an untrusted Home Manager user config.
- Predator: Freebox LAN IP is `192.168.1.1`; the Toshiba USB bridge aborts
  extended SMART tests after about 10 minutes, so use short tests and the
  monthly scrub.

## LAN machine map (`~/.ssh/config` aliases)

| alias    | address                               | purpose                                                        |
|----------|---------------------------------------|----------------------------------------------------------------|
| rpi      | 192.168.1.166                         | Raspberry Pi 4, Debian — hosts my PC power-control project     |
| laptop   | 192.168.1.61                          | Framework 13, Arch Linux — work laptop                         |
| main     | `mirsella.mooo.com:2222` (LAN `.131`) | main tower, Arch Linux — powerful desktop                      |
| predator | 192.168.1.19                          | NixOS — next home server (drive, Nextcloud, …), this repo's box |
