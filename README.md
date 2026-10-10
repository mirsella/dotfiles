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
Both desktops use zstd zram at 100% of RAM ahead of the swap partition, run weekly
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

Predator uses Lanzaboote for authenticated boot. Its OS PCR separator remains
disabled pending compatible TPM enrollments for both SSDs. The canonical
procedure, including signed-image and unlock-phase policies, is in
[Secure Predator TODO](secure-predator-todo.md). NixOS activation does not migrate
LUKS credentials; complete enrollment before the first separator-enabled reboot.

Arch's root-owned maintenance is installed separately on both workstations:

```sh
sudo python3 arch/maintenance/apply.py
```

On the Arch Btrfs workstations, `/nix` and `/var/lib/docker` mount from separate
`@nix` and `@docker` subvolumes, keeping their churn out of Timeshift root snapshots.

The installer preserves machine-specific Nix and Timeshift settings. It schedules
daily Nix GC with 7-day generation retention, two weekly and one monthly
Timeshift snapshot plus two pre-upgrade snapshots, a 1 GiB journal cap, weekly
pacman cache pruning, old Docker builder
cache pruning, and limited Btrfs maintenance. When `fail2ban` is installed, the
installer also enables its SSH jail: three failed logins ban an address for a
minute, then five, ten, twenty minutes for repeat offenders, doubling up to a
week, and the LAN is trusted. Arch
uses btrfs-progs' monthly scrub timer and its own balance and space-check units.
The desktop Home Manager profile caps the kache build cache at 150 GiB. NixOS
uses the equivalent shared Nix GC, journal and fail2ban settings, and the
desktops trim their encrypted SSDs weekly.

On Arch, Home Manager also prunes the user's Nix profiles daily with the same
7-day retention; the root timer cannot prune profiles in the user's XDG state
directory. Active profiles and their dependencies stay rooted, including main's
GGUF model. Builds can also trigger GC below 10 GiB free, targeting 20 GiB free.

## Workstation build memory

`workstation-oom-protect.service` runs the root-owned `host-tools oom-protect`
process policy on the desktops. Every 250 ms it sets `oom_score_adj=-900` for
mirsella's OpenCode, Rio and WezTerm executables. They remain eligible for kernel
OOM killing, but are strongly deprioritized. Compilers and WASM build tools get
`+800`; Cargo gets `+500`. The kernel considers memory usage along with these
adjustments, so they express a preference rather than a strict kill order.
The policy uses executable names, so changing a thread's name does not change
its priority. Newly started processes receive the policy on the next scan.

Linux inherits OOM scores across fork and exec. The policy resets inherited
`-900` scores to `+200` when a child runs another executable, so browsers,
shells and build scripts do not retain their parent's protection. This reserves
`-900` for the listed interactive executables within this user's processes.
Home Manager sets
`OOMPolicy=continue` and `ManagedOOMPreference=avoid` on OpenCode's service and
the Rio/WezTerm application templates. An OOM-killed child must not stop the
whole service; oomd prefers other cgroups if monitoring is enabled.

On Arch, the maintenance installer installs the policy. To install just this
service after building `host-tools`, without running storage maintenance:

```sh
sudo env HOST_TOOLS_BINARY=/path/to/host-tools python3 arch/maintenance/apply.py --oom-only
```

Apply the Home Manager profile for the user-service drop-ins. On NixOS, the
desktop system rebuild installs both the process policy and the drop-ins.

The NixOS desktop module sets zstd zram's logical capacity to 100% of RAM and
`SwapUsedLimit=95%`. It leaves root, system and user-wide oomd monitoring disabled.
Main's current Arch installation has the same zram and oomd settings in `/etc`,
backed up under `dotfiles/system/main/` by `dotfiles/update-system-backup.sh`.
The 95% threshold is dormant because no cgroups are opted into oomd monitoring.
On NixOS, activate through the system rebuild, not standalone Home Manager.
Zram size changes on the current Arch installation take effect on reboot; avoid
draining a full swap device during builds.

## Checks

```sh
nix flake check path:. --no-build --no-update-lock-file
```

This evaluates all NixOS and Arch Home Manager profiles and checks host isolation,
service lifecycle rules, and Predator's boot/storage invariants. Run the executable
unlock-generator and Arch maintenance tests with:

```sh
nix build path:.#checks.x86_64-linux.data-unlock path:.#checks.x86_64-linux.arch-maintenance --no-link
```

These tests use generated units and disposable files; they do not activate a system.

## OpenCode extensions

`pkgs/opencode-extensions` owns the local plugins, TUI extensions and their tests.
Its Nix build bundles dependencies and compiles the cache timer from TSX with a
locked toolchain. Home Manager installs the compiled files; chezmoi owns the JSON
settings. Build and test the extensions and the compiled idle watchdog with:

```sh
nix build path:.#opencode-extensions path:.#opencode-idle-watchdog --no-link
```

On Arch, activate Home Manager before applying updated TUI settings. On NixOS,
rebuild the system instead. Restart OpenCode to load the compiled plugins. The
watchdog keeps the `opencode-idle-watchdog` command and no longer compiles itself
on invocation.

## Agent prompts

OpenCode's editable files are the canonical prompts:

- Commands: `~/.config/opencode/commands/*.md`, tracked under
  `dotfiles/private_dot_config/opencode/commands/`.
- Skills: `~/.config/opencode/skills/*/SKILL.md`, tracked under
  `dotfiles/private_dot_config/opencode/skills/`.

Edit the live OpenCode files and save them with `chezmoi re-add`. After applying
the dotfiles on a device, generate its local Codex/Hermes adapters with:

```sh
nix run path:.#host-tools -- sync-agent-skills
```

This links `~/.agents/skills` to OpenCode's skills and writes command adapters in
`~/.codex/skills`. Both paths are ignored by chezmoi. The adapters read the live
OpenCode commands, so prompt-body edits take effect without copying them again.
Rerun the command after adding a command or changing its description. Restart
the agents to reload their skill lists.

## REA reverse engineering

The workstation Home Manager profiles install [REA](https://rea.tools/) and
Ghidra on Arch and NixOS. `pkgs/rea` pins the published CLI/MCP runtime and its
dependencies. Its wrappers select Nix-provided Node 24, Ghidra 12.1.x and JDK 21,
so OpenCode needs no shell environment setup or runtime package downloads.
Home Manager merges the `rea` MCP registration and local model provider into
`~/.config/opencode/opencode.json`; chezmoi owns the editable `opencode.jsonc`.
It also installs the matching `reverse-engineer-anything` skill and references.

The MCP server is disabled by default. Restart OpenCode after a rebuild, then
enable `rea` in its `/mcp` menu for an investigation. The CLI works independently:

```sh
rea doctor --provider ghidra --json
rea mcp doctor --json
rea function /absolute/path/to/program main --provider ghidra --json
```

Apply `homeConfigurations.main` or `homeConfigurations.laptop` on Arch. On NixOS,
use the system rebuild command above. Update this flake to change the installation;
`rea setup` writes client configuration outside Home Manager.

## Predator Proton Pass login

Hermes uses the owner's normal `pass-cli` login. The CLI keeps its session in
`~/.local/share/proton-pass-cli` and its local encryption key in the persistent
D-Bus Secret Service. `hermes-secret-service.service` unlocks that keyring at boot
using the SOPS-managed `hermes_keyring_password`.

To authenticate or recover a revoked session, run `pass-cli login` as `mirsella`
on Predator and open its URL in a browser. Check the login with `pass-cli info`.
The session survives reboots and the CLI handles token refresh; a scheduled
logout/login job is unnecessary.

Each device has its own login session and keyring. Keep the CLI's writable session
and database out of chezmoi and SOPS: a copied session file needs its matching
keyring key, and refreshed credentials would diverge between devices.

## Predator storage

`fast` is the separate 240 GB encrypted SSD. `tank` combines the Seagate 1 TB,
Toshiba 750 GB and WD 1 TB HDDs in one RAIDZ1 vdev, each inside LUKS.
Usable capacity is roughly 1.36 TiB before ZFS overhead. The pool tolerates one
failed HDD; replace it promptly and let ZFS resilver. `tank/archive` mounts at `/srv/data/archive` and
`tank/backup` at `/srv/backup`.

`hosts/predator/disks.nix` defines the physical drive identities shared by
boot, provisioning, unlocking and monitoring. `disko.nix` defines the data mount
layout consumed by the installed system without importing its formatter.
Native systemd crypttab units open data
disks in parallel by persistent disk ID. Only the OS disk unlocks in initrd;
data devices use bounded, noninteractive, optional unlocking. RAIDZ1 import can
continue when one HDD is missing. All data mounts use `legacy` and `nofail`, so
data-disk failures leave the OS and SSH available. Data-dependent services require
their mounts and refuse the bare OS filesystem. Preserve
the keys under `/etc/luks/` when reinstalling. `disko.nix` is only for a fresh
installation and formats the SSD and all three HDDs.

SMART checks all five physical disks, runs daily short self-tests at noon and
weekly long tests on Sunday: SSDs and WD at 13:00, Seagate at 14:00 and Toshiba
at 15:00. Temperature emails start at 55 C for HDDs and 65 C for SSDs.
SMART sends warnings through Resend to
`mirsella@protonmail.com`. USB disks can reconnect
without stopping monitoring of the other disks. ZED emails device faults,
I/O errors, checksum errors and scrub failures using the same Resend key as
Nextcloud. The kernel-journal watcher also sends USB connection errors to Telegram.
Both pools receive a monthly scrub; Sanoid retains daily and weekly snapshots.
Snapshots and database dumps on `tank` do not survive loss of that pool.

Syncoid replicates `fast/data` and `fast/ncdata` to `tank/replica/data` and
`tank/replica/ncdata` nightly at 23:45. Replicas are read-only and unmounted;
Sanoid prunes their daily and weekly snapshots without taking destination
snapshots. Syncoid also takes a fresh synchronization snapshot for each run.
The overnight idle check waits for replication and recovery jobs to finish.

`recovery-backup.service` runs at 23:10 and retains 14 root-only bundles in the
separate `tank/backup/recovery` dataset at `/srv/backup/recovery/`; `latest`
points to the newest complete bundle. Each
contains all five current LUKS headers and UUIDs plus `system-secrets.tar` with
`/etc/luks/`, the SOPS SSH host identity and `/var/lib/sbctl`. These archives
contain private keys. Restore them as root with their original permissions.

`db-backup.service` uses the same private dataset, publishing database dumps and
Nextcloud configuration under `/srv/backup/recovery/db/latest` at 23:00. Both
backup jobs stage complete bundles, atomically publish `latest`, and retain 14
bundles. Before deploying this path change, move any existing dumps from
`/srv/backup/db/` into the protected `db/` directory.

`fast/tank-pre-raidz1-20261003` holds a read-only, unmounted migration copy,
including the older snapshots. It is a fixed recovery point and does not track
later writes.

SMART lifetime counters cannot generally be cleared. The post-cable baseline and
migration snapshot manifest are saved under
`/var/lib/storage-baselines/post-cable-20261003/` on Predator. Compare future
readings with these values to distinguish new errors from the drive's history.

## Predator overnight suspend

The timer checks every minute from 00:30 through 06:59 local time. It requires
30 minutes without authenticated application activity, no logged-in SSH/local
users, and no active maintenance or storage blocker. It warms the USB disks,
sets a 07:00 RTC alarm and requests deep suspend with systemd inhibitors enabled.

`pkgs/host-tools/src/activity.rs` recognizes Nextcloud's response-side
`X-User-Id` header and successful protected API requests for Immich, OpenCode
and OpenChamber. Valid API keys and authorized shared-photo access also count.
Public pages, health checks, failed logins, scanner requests and unauthenticated
SSH connections do not reset the quiet period. Keeping an app logged in without
requests does not count as activity by itself.

Caddy's connections to the actual app backends also block suspend while a
transfer or event stream may be in progress. This guard is deliberately broader
than authenticated completed requests: a request's authentication result is not
available in the access log until it finishes. Short backend keepalives can
therefore delay a check, but do not reset the 30-minute clock. Public listening
sockets and local container health checks do not trigger this guard.

Inspect decisions with `sudo journalctl -u night-suspend.service`. Blocked checks
name the app, login, proxy connection or maintenance job. Run the activity-only
diagnostic with:

```sh
sudo host-tools night-suspend --activity-only
CARGO_TARGET_DIR="$HOME/dev/host-tools-target" cargo test --locked --manifest-path pkgs/host-tools/Cargo.toml
```

The protected API route lists need review when upgrading these applications or
changing Caddy's hostnames and upstreams.

## Custom service programs

`pkgs/host-tools` contains the Rust implementations of our recurring host jobs.
Nix and systemd define their schedules, dependencies, users and permissions.
The subcommands cover overnight suspend, storage alerts and mail, database and
recovery backups, Beszel and Nextcloud setup, lid/backlight handling, Ryzen power
limits, Bluetooth audio profiles, Btrfs headroom checks, Wake-on-LAN and netconsole.

Build the Nix package with `nix build path:.#host-tools`. A native build for Arch
or Debian is also supported:

```sh
CARGO_TARGET_DIR="$HOME/dev/host-tools-target" cargo build --release --locked --manifest-path pkgs/host-tools/Cargo.toml
sudo HOST_TOOLS_BINARY="$HOME/dev/host-tools-target/release/host-tools" python3 arch/maintenance/apply.py
```

The Arch installer copies that binary to `/usr/local/libexec/host-tools` for its
system units. On the Pi, build for its architecture, install the binary as
`/usr/local/bin/host-tools`, and use the units in `aux/rpi`. Replace the old cron
entry when enabling the WoL timer; do not schedule both. Keep that timer disabled
during RTC-only wake comparisons.

Tests for the recurring programs live in `pkgs/host-tools/src/tests.rs` and run
as part of the Nix package build. Python remains only in manual/one-time tools
and their tests; upstream application internals are managed by their packages.

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
