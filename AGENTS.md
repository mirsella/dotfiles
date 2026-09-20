Declarative NixOS config for **predator**.

Deploy from another machine looks like:
`rsync -a --delete . predator:~/dev/nixos/ && ssh predator 'sudo nixos-rebuild switch --flake /home/mirsella/dev/nixos#predator'`
(repo lives at `~/dev/nixos` on every machine; chezmoi `sourceDir` points there. Long rebuilds: launch detached, poll the log.)

Manual files (kept outside this repo):
- laptop `/etc/systemd/logind.conf.d/nolidsleep.conf` (ignore lid switch), laptop `~/.config/powerdevilrc` (`LidAction=64` = screen off on lid close, all profiles)
- Arch boxes: system Caddy (`/etc/caddy/Caddyfile`, system `caddy.service`), udev rules, pacman hooks
- predator: Freebox LAN IP is `192.168.1.1` (not factory `.254`); `hd-idle` must be stopped during SMART long tests or it spins the disk down mid-test (every rebuild restarts it, so stop it again right after). The Toshiba USB bridge aborts extended self-tests ~10 min in regardless; rely on short tests plus the monthly scrub.

## LAN machine map (`~/.ssh/config` aliases)

| alias    | address                               | purpose                                                        |
|----------|---------------------------------------|----------------------------------------------------------------|
| rpi      | 192.168.1.166                         | Raspberry Pi 4, Debian — hosts my PC power-control project     |
| laptop   | 192.168.1.61                          | Framework 13, Arch Linux — work laptop                         |
| main     | `mirsella.mooo.com:2222` (LAN `.131`) | main tower, Arch Linux — powerful desktop                      |
| predator | 192.168.1.19                          | NixOS — next home server (drive, Nextcloud, …), this repo's box |
