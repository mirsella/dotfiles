# Machine configuration

The flake discovers host directories under `hosts/` and uses each directory name
as the hostname. NixOS and standalone Arch Home Manager profiles share the same
workstation home configuration.

Every host directory exports a complete `nixosModules.<hostname>` and
`nixosConfigurations.<hostname>`. Its hardware file is required; `home.nix` also
exposes the standalone Arch `homeConfigurations.<hostname>`.

| Host | NixOS role | Machine-specific settings |
| --- | --- | --- |
| `main` | Plasma desktop | CoolerControl fan curve and NCT6798 driver |
| `laptop` | Plasma desktop | Framework RyzenAdj limits and Bluetooth audio watcher |
| `predator` | Headless server | Secure Boot, encrypted storage, ZFS, server services and scheduled suspend |

- `modules/nixos/common.nix` owns the shared user, networking, Nix settings and Home Manager integration.
- `modules/nixos/desktop.nix` owns Plasma, audio, Bluetooth and power profiles.
- `hosts/<hostname>/default.nix` selects roles and holds machine-specific system settings.
- `hosts/<hostname>/home.nix` holds workstation home settings, including the Git signing key.
- `modules/home/` contains shared home settings, NixOS packages and workstation services.
- `hardware-configuration.nix` records the installed disk layout and detected hardware.

## Install main or laptop

All three install targets are available. Main and laptop's hardware configurations
were generated from their live Arch installations using the pinned NixOS scanner.
They record the existing Btrfs subvolumes and EFI partitions; laptop also records
its LUKS root mapping and the migrated `@nix` mount. Main's `@nix` entry should
only be added after its one-time Arch subvolume migration creates it. Host modules
preserve compression, existing swap files, tmpfs sizes, laptop's TPM unlock option
and main's NTFS data mount. Both desktops
use zstd zram at 50% of RAM, ahead of disk swap.

From the NixOS installer, mount the intended root and boot filesystems under
`/mnt`. On laptop, also mount the existing `@nix` subvolume at `/mnt/nix` before
installing so the store is populated on the same subvolume used after boot. Then
run from this checkout. Select `main` or `laptop`:

```sh
host=main
sudo nixos-install --flake "path:$PWD#$host"
```

If repartitioning or recreating filesystems, regenerate the hardware file after
mounting the new layout and review storage settings in the host's `default.nix`:

```sh
sudo nixos-generate-config --root /mnt --show-hardware-config > "hosts/$host/hardware-configuration.nix"
```

Use `path:` while the hardware file is untracked. These commands do not partition
or format disks. `disko.nix` is Predator's separate, destructive provisioning
layout and does not apply to the desktops.

Preserve `~/.ssh/id_ed25519` and the host's GPG signing key when reinstalling.
Workstation SOPS secrets use the user's SSH key; Predator uses its system SSH
host key. Home Manager owns the secret paths and user services on the desktops.

## Rebuild

On an installed NixOS machine, `nixos-rebuild` selects the current hostname when
the flake reference has no fragment:

```sh
sudo nixos-rebuild switch --flake path:/home/mirsella/dev/dotfiles
```

For the current Arch installations, use `homeConfigurations.main` or
`homeConfigurations.laptop` with standalone Home Manager. Pacman/AUR still own
their application binaries. NixOS supplies those binaries from Nix packages.
Chezmoi continues to own editable application dotfiles; NixOS installation and
fan configuration do not run chezmoi hooks.

Arch's root-owned maintenance is installed separately on both workstations:

```sh
sudo python3 arch/maintenance/apply.py
```

On an existing Arch installation with `/nix` and `/var/lib/docker` still inside
the root subvolume, run `sudo python3 arch/maintenance/migrate-subvolumes.py`
once. It stops the two daemons, copies their state to new Btrfs subvolumes, adds
fstab mounts, then starts the daemons. It retains the `.before-subvolume`
directories for rollback; the job below checks the new mounts and services
before deleting them on a later boot. Run the migration separately on each Arch
Btrfs workstation. NixOS uses Podman
instead of Docker, so its hardware file needs the `@nix` mount but not an
automatic `/var/lib/docker` mount.

To remove the retained rollback copies on the next boot, without touching them
in the current session, arm the one-shot service after migration:

```sh
sudo install -Dm644 arch/maintenance/cleanup-rollback.py /usr/local/libexec/cleanup-rollback.py
sudo install -Dm644 arch/maintenance/arch-rollback-cleanup.service /etc/systemd/system/arch-rollback-cleanup.service
sudo touch /run/arch-rollback-cleanup-defer
sudo systemctl daemon-reload
sudo systemctl enable arch-rollback-cleanup.service
```

It checks both subvolume mounts and the Nix and Docker daemons before removing
the old directories. It disables itself on success and stays enabled for another
boot if a check fails. Review `journalctl -u arch-rollback-cleanup.service` after
reboot. A Timeshift snapshot can retain the old blocks until that snapshot expires.

The installer preserves machine-specific Nix and Timeshift settings. It schedules
14-day Nix GC, two weekly and one monthly Timeshift snapshot plus two pre-upgrade
snapshots, a 1 GiB journal cap, weekly pacman cache pruning, old Docker builder
cache pruning, and limited Btrfs maintenance. Arch uses btrfs-progs' monthly
scrub timer. The desktop Home Manager profile caps the kache build cache at
150 GiB. NixOS uses the equivalent shared Nix GC and journal settings; its
Btrfs desktops also scrub monthly and check space daily. At 22:00, a root job
checks unallocated device space; below 8 GiB it runs bounded, limited data
balances to reach 12 GiB. The root check logs a failed unit and the desktop
timer notifies at 90% Btrfs metadata usage or below 8 GiB unallocated,
regardless of ordinary disk free space. Inspect
`systemctl status btrfs-balance-limited.service btrfs-space-check.service` and
`btrfs filesystem usage /` if reclamation fails.

## Checks

```sh
nix flake check path:. --no-build --no-update-lock-file
```

This evaluates all NixOS and Arch Home Manager profiles and checks host isolation,
service lifecycle rules, and Predator's boot/storage invariants. The checks are
pure evaluations and do not activate a system or run the disk formatter.

## Main's fan configuration

Only `hosts/main/default.nix` enables CoolerControl. It loads `nct6775` and installs
`dotfiles/system/main/etc/coolercontrol/config.toml` as a writable root-owned
`/etc/coolercontrol/config.toml` before starting the daemon. Systemd creates the
configuration directory. A changed TOML produces a changed service unit, so
`nixos-rebuild switch` stops the old daemon before installing the new config.

Unrelated rebuilds leave the running daemon and its config alone. GUI changes
last until the daemon restarts; copy them back into the tracked TOML to keep them.
The saved curve stays at 20% through 65°C, reaches 50% at 70°C, stays there through
85°C, then reaches 100% at 90°C. CoolerControl interpolates the ramps.
