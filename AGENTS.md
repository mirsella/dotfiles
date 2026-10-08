# Repository workflow

The checkout is `~/dev/dotfiles` on every machine. `.chezmoiroot` selects
`dotfiles/` as the chezmoi source. `hosts/<hostname>/default.nix` selects each
machine's NixOS role; `home.nix` adds its workstation Home Manager settings.
The flake discovers these directories and sets hostnames automatically.

- Put system setup, packages, service units, timers and their configuration in
  Nix/NixOS modules; use Home Manager for the user settings it owns. Reproduce
  the setup from this flake instead of relying on imperative machine edits.
- Write custom daemons, automation and administrative scripts in Rust. Use
  `pkgs/host-tools` for shared host operations and separate packages for application
  runtimes. Keep unavoidable adapters to an upstream API in its required language
  small, with the operational logic in Rust.
- Nightly Cargo's `-Zscript` is available for small Rust scripts and one-off
  tasks. Deployed services and scheduled scripts must be compiled during the
  Nix build with pinned dependencies, not on their first invocation. They should
  start without a compiler, network dependency downloads or writable build caches.
- `main` and `laptop` share the Plasma desktop module; `predator` is headless.
  All three share `modules/nixos/common.nix`. Keep hardware and machine-specific
  settings in their host directory, not hostname branches in shared modules.
- Each host directory becomes a complete `nixosModules.<hostname>` and
  `nixosConfigurations.<hostname>`, including its required `hardware-configuration.nix`.
  See `README.md` for installation and rebuild commands.
- Main's CoolerControl service installs the tracked TOML before each daemon start.
  Config changes change the systemd unit and trigger a restart on rebuild.
  Save permanent curve edits in `dotfiles/system/main/etc/coolercontrol/config.toml`.
- Chezmoi owns editable app configuration. Home Manager owns packages, generated
  Git/SSH settings, user services and SOPS wiring. Keep each path under one
  owner; `.chezmoiignore` lists the HM-owned paths.
- OpenCode extensions and their tests live in `pkgs/opencode-extensions`; Home
  Manager installs the compiled plugins. Its editable JSON settings remain in
  chezmoi. The idle watchdog is compiled from `pkgs/opencode-idle-watchdog`.
- Retire one-time migrations after every affected host has completed them. Keep
  deployment evidence in Git history rather than permanent acceptance reports.
- Chezmoi auto-commits and auto-pushes source edits. Its Git operation can include
  unrelated changes in this shared repository, including staged Nix changes.
  Review or finish those changes before `chezmoi add`, `edit` or `re-add`.
- On Arch, pacman/AUR own application binaries; the HM profiles install config
  and services, and kache from the flake so the daemon and cargo wrapper always
  match. On NixOS, rebuild the system target, never activate a standalone
  HM profile for the same user. The standalone homes remain available for Arch.
- Before deploying to Predator, inspect its `~/dev/dotfiles` status and diff,
  especially `flake.lock` (the weekly upgrade updates nixpkgs there). Sync only
  reviewed changes; do not use whole-tree `rsync --delete` over concurrent work.
  Rebuild with `ssh predator 'sudo nixos-rebuild switch --flake path:/home/mirsella/dev/dotfiles#predator'`.
  Run long rebuilds detached and poll the unit log and exit status.
- `nix flake check path:. --no-build --no-update-lock-file` evaluates all profiles
  and runs the host-isolation and Predator boot/storage invariants through `checks`.
  Run focused Cargo tests for monitoring, backups and suspend changes. The
  `data-unlock` and `arch-maintenance` checks run the tests under `tests/` during
  `nix flake check` without `--no-build`.
- `disko.nix` formats **all four data disks** only for a fresh, intentional
  installation. To reinstall on existing disks, mount them without running disko,
  preserve the LUKS headers, ZFS pools, Secure Boot signing bundle (`/var/lib/sbctl`),
  HDD keys (`/etc/luks/`), and SOPS host identity (`/etc/ssh/ssh_host_ed25519_key`),
  then install `#predator`.
  The database dumps and ZFS snapshots on `tank/backup` are on-machine only.
  `tank` uses three encrypted HDDs in RAIDZ1 and tolerates one failed HDD.
  The read-only migration copy `fast/tank-pre-raidz1-20261003` is a fixed recovery point on the separate
  SSD, not an ongoing backup. SMART and ZED send storage warnings via Resend.
- Syncoid replicates the two live SSD datasets to read-only, unmounted
  `tank/replica/{data,ncdata}` at 23:45. Recovery bundles run at 23:10 under
  `/srv/backup/recovery/latest`: current LUKS headers and a private archive of
  HDD keys, the SOPS host identity and Secure Boot signing keys. Keep this directory root-only.
  Database dumps use `/srv/backup/recovery/db/latest` on the same private dataset;
  both jobs publish atomically and retain 14 complete bundles. Existing dumps in
  `/srv/backup/db/` need moving before deploying the new path.
- Data disks unlock through optional runtime crypttab units, not initrd. All data
  datasets use systemd-managed legacy mounts with `nofail`. Keep missing data disks
  nonfatal to boot, allow degraded RAIDZ1 imports, and gate writers on their mounts.

Manual files outside the repo:
- Laptop uses Plasma lid defaults; Predator ignores lid-close via
  `hosts/predator/default.nix` (screen off instead of sleep).
- Arch boxes: root-owned Nix daemon GC/cache settings, Timeshift retention,
  storage timers and the fail2ban SSH jail are installed from
  `arch/maintenance/apply.py`. System
  Caddy (`/etc/caddy/Caddyfile`, `caddy.service`), udev rules and pacman hooks
  remain manual. `pacman.conf` also pins `IgnorePkg = openchamber` so the AUR
  package stays on 1.x with OpenCode 1.x. The Nix daemon ignores cache settings
  from an untrusted Home Manager user config.
- Predator: Freebox LAN IP is `192.168.1.1`; SMART runs daily short and weekly
  long tests on all five physical drives. Toshiba's earlier USB setup had repeated
  host-aborted long tests after about 10 minutes; check its adapter, cable and power
  if this recurs.
- rpi (Debian) is outside the flake; its fail2ban SSH jail is tuned by hand in
  `/etc/fail2ban/jail.d/sshd.local` with the same values as the workstations.

## LAN machine map (`~/.ssh/config` aliases)

| alias    | address                               | purpose                                                        |
|----------|---------------------------------------|----------------------------------------------------------------|
| rpi      | 192.168.1.166                         | Raspberry Pi 4, Debian — hosts my PC power-control project     |
| laptop   | 192.168.1.61                          | Framework 13, Arch Linux — work laptop                         |
| main     | `mirsella.mooo.com:2222` (LAN `.131`) | main tower, Arch Linux — powerful desktop                      |
| predator | 192.168.1.19                          | NixOS — next home server (drive, Nextcloud, …), this repo's box |

## SSH public keys (user identity)

`~/.ssh/id_ed25519.pub` of each machine, to copy into an `authorized_keys`
file, a service account or a Git host. Main, laptop and predator are also
registered on the GitHub account; rpi is not.

| device | public key | SHA256 fingerprint |
|---|---|---|
| main | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe` | `SHA256:Mcc/nTQqdEAlOiPFRj6yHYf4exhB/iM5CwoxcckKpPg` |
| laptop | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF` | `SHA256:Y8Xmd8d+/SfbsiyozITkC13MVP5MKrwlllz1cw58z4E` |
| predator | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHALDtwhSj8AF8NdRRjcWcDkD/EeDnRcWYjRSbXSznTo` | `SHA256:/6MNKC1SnVco8IlGO6Xe3FDh0BWlE6U7y+fPrAUK82c` |
| rpi | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJRadFVhpJRbD98sHFgcVAaMDL48x27On4XAe//Yx6Vm` | `SHA256:4vEJhi7Q5OOw8M4jArYQLG/0x4EID933Ohxsc0Jf6vQ` |
