Dotfiles (chezmoi source under `dotfiles/`, `.chezmoiroot`) plus declarative NixOS config for **predator**.

Edit in `~/dev/dotfiles`. Predator's configured rebuild checkout is still
`~/dev/nixos`; check its local changes and copy only reviewed files. Avoid
whole-tree `rsync --delete` over concurrent work. After syncing, rebuild with:
`ssh predator 'sudo nixos-rebuild switch --flake path:/home/mirsella/dev/nixos#predator'`
The explicit path flake lets root build the user-owned checkout. Launch long
rebuilds detached and poll their logs.

Manual files (kept outside this repo):
- laptop uses plasma defaults (sleep on lid close), no special lid config; predator ignores the lid switch via `configuration.nix` logind settings (screen off instead of sleep)
- Arch boxes: system Caddy (`/etc/caddy/Caddyfile`, system `caddy.service`), udev rules, pacman hooks
- predator: Freebox LAN IP is `192.168.1.1` (not factory `.254`); `hd-idle` must be stopped during SMART long tests or it spins the disk down mid-test (every rebuild restarts it, so stop it again right after). The Toshiba USB bridge aborts extended self-tests ~10 min in regardless; rely on short tests plus the monthly scrub.

## LAN machine map (`~/.ssh/config` aliases)

| alias    | address                               | purpose                                                        |
|----------|---------------------------------------|----------------------------------------------------------------|
| rpi      | 192.168.1.166                         | Raspberry Pi 4, Debian — hosts my PC power-control project     |
| laptop   | 192.168.1.61                          | Framework 13, Arch Linux — work laptop                         |
| main     | `mirsella.mooo.com:2222` (LAN `.131`) | main tower, Arch Linux — powerful desktop                      |
| predator | 192.168.1.19                          | NixOS — next home server (drive, Nextcloud, …), this repo's box |
