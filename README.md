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
- `modules/nixos/desktop.nix` owns Plasma, audio, Bluetooth, power profiles and the shared encrypted root layout.
- `hosts/<hostname>/default.nix` selects roles and holds machine-specific system settings.
- `hosts/<hostname>/home.nix` holds workstation home settings, including the Git signing key.
- `modules/home/` contains shared home settings, NixOS packages and workstation services.
- `hardware-configuration.nix` records each machine's detected hardware and EFI partition.

## Install main or laptop

Both desktops install onto a LUKS2-encrypted XFS root with TPM2 auto-unlock and a
16 GiB encrypted swap partition used for hibernation. Main keeps its EFI and
Windows partitions; only the old Linux partition is replaced. Laptop keeps its EFI
partition. The single XFS filesystem holds `/`, `/home` and `/nix`;
`modules/nixos/desktop.nix` records the LUKS containers and the resume device.
Both desktops use zstd zram at 50% of RAM ahead of the swap partition, run weekly
`fstrim` through the encrypted root, and main keeps its NTFS data mount.

Partitioning is destructive for the Linux partition only. Delete it, create a
16 GiB swap partition plus one large root partition, then create the containers
and the filesystem:

```sh
# main: /dev/nvme0n1, old Linux partition p2; p1 (EFI) and p3-p5 (Windows) stay.
sudo sgdisk --delete=2 /dev/nvme0n1
sudo sgdisk --new=2:0:+16G --change-name=2:nixos-swap /dev/nvme0n1
sudo sgdisk --new=6:0:0 --change-name=6:nixos-root /dev/nvme0n1

# laptop: old LUKS root is p2; p1 (EFI) stays, the new root partition is p3.
sudo sgdisk --delete=2 /dev/nvme0n1
sudo sgdisk --new=2:0:+16G --change-name=2:nixos-swap /dev/nvme0n1
sudo sgdisk --new=3:0:0 --change-name=3:nixos-root /dev/nvme0n1

sudo cryptsetup luksFormat --type luks2 /dev/disk/by-partlabel/nixos-swap
sudo cryptsetup luksFormat --type luks2 /dev/disk/by-partlabel/nixos-root
sudo cryptsetup open /dev/disk/by-partlabel/nixos-swap cryptswap
sudo cryptsetup open /dev/disk/by-partlabel/nixos-root cryptroot
sudo mkswap /dev/mapper/cryptswap
sudo mkfs.xfs /dev/mapper/cryptroot
```

Mount the new root under `/mnt` and install from this checkout. Use the existing
EFI UUIDs: main `3613-BAFF`, laptop `F513-ECCB`.

```sh
sudo mount /dev/mapper/cryptroot /mnt
sudo mkdir /mnt/boot
sudo mount /dev/disk/by-uuid/3613-BAFF /mnt/boot
host=main
sudo nixos-install --flake "path:$PWD#$host"
```

On the first boot, enter the LUKS passphrase when prompted, then enroll the TPM so
later boots unlock without it. `--tpm2-pcrs=7` binds the key to the secure-boot
state, so firmware updates keep working and only secure-boot changes re-prompt.

```sh
sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 /dev/disk/by-partlabel/nixos-swap
sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 /dev/disk/by-partlabel/nixos-root
```

Use `path:` while the hardware file is untracked. `disko.nix` is Predator's
separate, destructive provisioning layout and does not apply to the desktops.

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
Btrfs workstation. Reinstalling a desktop on the XFS layout replaces that root,
so its old migrations and snapshots go away with it.

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
cache pruning, and limited Btrfs maintenance. When `fail2ban` is installed, the
installer also enables its SSH jail: three failed logins ban an address for a
minute, then five, ten, twenty minutes for repeat offenders, doubling up to a
week, and the LAN is trusted. Arch
uses btrfs-progs' monthly scrub timer and its own balance and space-check units.
The desktop Home Manager profile caps the kache build cache at 150 GiB. NixOS
uses the equivalent shared Nix GC, journal and fail2ban settings, and the
desktops trim their encrypted SSDs weekly.

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
