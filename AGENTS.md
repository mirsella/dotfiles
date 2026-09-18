Declarative NixOS config for **predator**.

Working on predator from another machine looks like:
`scp -r . predator:~/nixos && ssh predator 'sudo nixos-rebuild switch --flake /home/mirsella/nixos#predator'`

Manual system-level files (kept outside Home Manager, Arch boxes only):
- system Caddy (`/etc/caddy/Caddyfile`, system `caddy.service`), udev rules, pacman hooks

## LAN machine map (`~/.ssh/config` aliases)

| alias    | address                               | purpose                                                        |
|----------|---------------------------------------|----------------------------------------------------------------|
| rpi      | 192.168.1.166                         | Raspberry Pi 4, Debian — hosts my PC power-control project     |
| laptop   | 192.168.1.61                          | Framework 13, Arch Linux — work laptop                         |
| main     | `mirsella.mooo.com:2222` (LAN `.131`) | main tower, Arch Linux — powerful desktop                      |
| predator | 192.168.1.19                          | NixOS — next home server (drive, Nextcloud, …), this repo's box |
