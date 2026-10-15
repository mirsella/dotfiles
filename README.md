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
git add "hosts/$host/hardware-configuration.nix"
sudo nixos-install --flake ".#$host"
```

On the first boot, enter the LUKS passphrase when prompted, then enroll the TPM so
later boots unlock without it. `--tpm2-pcrs=7` binds the key to the secure-boot
state, so firmware updates keep working and only secure-boot changes re-prompt.

```sh
sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 /dev/disk/by-partlabel/nixos-swap
sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 /dev/disk/by-partlabel/nixos-root
```

`disko.nix` is Predator's separate, destructive provisioning layout and does not
apply to the desktops.

Preserve `~/.ssh/id_ed25519` and the host's GPG signing key when reinstalling.
Workstation SOPS secrets use the user's SSH key; Predator uses its system SSH
host key. Home Manager owns the secret paths and user services on the desktops.

## Rebuild

On an installed NixOS machine, `nixos-rebuild` selects the current hostname when
the flake reference has no fragment:

```sh
sudo nixos-rebuild switch --flake /home/mirsella/dev/dotfiles
```

For the current Arch installations, use `homeConfigurations.main` or
`homeConfigurations.laptop` with standalone Home Manager. Pacman/AUR still own
their application binaries, except the writable agent installs below. NixOS
supplies other binaries from Nix packages.
Chezmoi continues to own editable application dotfiles; NixOS installation and
fan configuration do not run chezmoi hooks.

### OpenCode, Codex and T3 Code

All three hosts import `modules/home/agent-tools.nix`. Home Manager runs the
Nix-built `bootstrap-agent-tools` helper during activation. It installs the
official standalone OpenCode v2 CLI, Codex CLI (latest stable), T3 CLI (nightly),
and a separate nightly T3 desktop AppImage. To apply just this bootstrap without
switching the rest of a host's configuration:

```sh
mkdir -p ~/.local/state/agent-tools
nix build .#agent-tools --out-link ~/.local/state/agent-tools/bootstrap
~/.local/state/agent-tools/bootstrap/bin/bootstrap-agent-tools
```

On NixOS, use the Home Manager module's bootstrap package, which includes the
AppImage runtime, instead of the default Arch launcher:

```sh
nix build \
  .#nixosConfigurations.predator.config.home-manager.users.mirsella.programs.agent-tools.package \
  --out-link ~/.local/state/agent-tools/bootstrap
```

Then run the same bootstrap helper above. Bootstrap registers
`~/.local/state/agent-tools/installed` as a Nix GC root. The desktop launchers and
service drop-in use that stable path. Rebuilding or removing the build out-link
does not unprotect the installed package. Bootstrap updates the installed root
only after the native installs succeed; it does not retain old package roots.

Nix builds the Rust helpers and caches package outputs. Cargo compilation
directories are temporary, not part of the installed package. For interactive
Cargo work, use the crate's ignored `pkgs/<crate>/target/` directory.

OpenCode lives at `~/.opencode/bin/opencode`, linked from `~/.local/bin`.
Codex lives under `~/.codex/packages/standalone`, and T3's CLI releases under
`~/.t3/runtime/versions`. The desktop AppImage is a writable file at
`~/.local/share/t3code/app/T3-Code-nightly.AppImage`. Activation keeps existing
native installs; `opencode upgrade`, `codex update`, `t3 update`, and the desktop's updater own
subsequent releases. The initial CLI and desktop share a pinned nightly version;
updating that seed only affects fresh installs.

Nix supplies the `t3code`/`t3code-nightly` launchers and desktop menu entry. On
NixOS the launcher uses `appimage-run`; Arch runs the AppImage directly. A service
drop-in puts `~/.local/bin` first in T3's PATH while preserving existing shell
exports. Downloads use disk-backed scratch space under `~/dev`. T3 owns its
background-service unit and updater state. Bootstrap installs the T3 service
when a user manager is available; setup finishes on the next user login otherwise.
Home Manager starts `agent-tools-setup.service` after activation. It waits for
T3's runtime file and reconciles the editable portable preferences with live
T3 settings through T3's project API, in either direction. Changes to the
portable bundle, OpenCode's service credential, T3's runtime file, or any
live T3 settings file trigger the same idempotent sync.

Chezmoi stores portable T3 preferences and project registrations in
`~/.config/t3code/portable-settings.json`. The Nix-built `t3-settings` helper
captures fields listed in `pkgs/host-tools/src/t3-settings-allowlist.json`.
It excludes credentials, provider environment variables, browser profiles,
saved connections, onboarding state and window geometry. Raw `~/.t3` and
Electron profile directories are ignored by chezmoi. Conversations and
attachments need a separate backup.
T3's OpenCode provider omits `serverUrl` so T3 launches its own server
and injects its thread-scoped MCP tools. The standalone OpenCode service remains
available to other clients. If an instance explicitly uses the local singleton
on port 14096, apply reads its password from `~/.config/opencode/service.json`
into local T3 settings, and rotating that credential triggers a sync that
re-injects it even when every other setting already matches. The password
stays out of portable captures and Git.
Other endpoints keep their own credentials. GitHub device authorization and
agent-provider logins remain device-local.
Capture and apply report unknown field paths without printing their values.
Add a field to the allowlist to capture it, or set its rule to `false` to
explicitly ignore it. Known exclusions do not produce warnings.

Preferences changed in the app sync back to the portable bundle automatically:
the settings watcher reruns the sync when live T3 settings files change, and
the one side that changed wins. When both sides changed since the last sync,
the sync does nothing and the service fails with both diffs; resolve it with
`t3-settings sync --force-capture` (live settings win) or
`t3-settings sync --force-apply` (bundle wins). A machine with no previous
sync record (fresh onboarding) instead adopts the bundle after backing the
live settings up. Every overwrite, adoption, and conflict
is archived with before/after snapshots under
`~/.local/state/agent-tools/t3-settings-history/` for recovery, and the
journal shows the diff. Preview any decision with
`t3-settings sync --dry-run`.

To commit the synced bundle to Git:

```sh
chezmoi re-add ~/.config/t3code/portable-settings.json
git diff -- dotfiles/private_dot_config/t3code/private_portable-settings.json
```

The `re-add` pre-hook runs `t3-settings capture --if-configured` to refresh the
local portable bundle before chezmoi copies it into the source. Machines without
T3 userdata skip capture with a diagnostic; capture errors stop `re-add`.
The hook also runs for targeted and dry-run invocations: it refreshes the local
bundle, while chezmoi controls which files are saved. To save only T3 settings,
use `chezmoi re-add ~/.config/t3code/portable-settings.json`.
Chezmoi's usual automatic commit and push apply. Review other repository changes
before `re-add`; the captured bundle includes project script commands and URLs.
Do not `chezmoi add` the live T3 settings files.

Completed migration backups under `~/dev/opencode-v1-backups` can be enrolled
with `host-tools agent-tools-backups enroll <bundle-directory>`. The weekly
Home Manager timer keeps the newest two enrolled bundles and every enrolled
bundle younger than 30 days. It prunes complete bundle directories together;
unenrolled directories and symlinks are excluded. Preview retention with
`host-tools agent-tools-backups prune --dry-run`.

`t3-settings capture` can refresh the local bundle separately without committing
or pushing. Home Manager installs its launcher at `~/.local/bin/t3-settings`.
Before the next activation, build the bootstrap package above and create the
same launcher link if it is absent:

```sh
ln -s ~/.local/state/agent-tools/installed/bin/t3-settings ~/.local/bin/t3-settings
```

Apply `~/.config/chezmoi/chezmoi.toml` to install the hook on an existing machine.

On another machine, clone the project checkouts, start the T3 server, then:

```sh
chezmoi apply ~/.config/t3code/portable-settings.json
t3-settings apply --dry-run
t3-settings apply
```

Apply merges allowed fields and preserves local credentials and unlisted
settings outside lists. List settings, including keybindings, favorites,
model options and project scripts, are replaced in full; unknown fields
inside their items are not preserved. It clears saved preferences that were
reset to defaults. Project
paths under the source user's home become `~/...`; registration uses T3's
project API and maps overrides to the destination IDs. Project settings use
T3's canonical overrides; derived legacy views stay local. Missing checkouts
are skipped with a warning; clone them and apply again. Existing conversations and
projects outside the bundle are preserved. Saved server exposure and local
environment preferences apply too. Close the settings editor while restoring,
then restart the desktop to reload client preferences. Neither command clones
repositories or restores authentication. Connections must be paired locally.

### Laptop authenticated boot (Arch)

`arch/laptop-boot.nix` supplies the root-owned mkinitcpio presets, UKI signing
configuration, embedded command line and systemd-boot defaults. Home Manager
exposes the generated source at `~/.local/share/arch-uki`. Pacman owns mkinitcpio,
ukify and the kernels; its existing hooks rebuild and sign each UKI on updates.
The signing keys stay on the encrypted root filesystem, outside the Nix store.
The PCR signing key uses RSA-2048, supported by laptop's Pluton TPM. The
refresh command tests loading the public key into the TPM before rebuilding images.

Apply changes and verify each generated image with:

```sh
sudo /usr/local/libexec/host-tools arch-uki ~/.local/share/arch-uki
```

The policy pins Secure Boot state through PCR 7 and accepts signed PCR 11 values
only for the `enter-initrd` phase with policy reference `initrd`. Kernel updates
receive new signatures from the same PCR signing key and do not require
re-enrollment. The passphrase slot remains available for recovery.

The refresh command uses the generated manifest to locate installed kernels,
checks the existing signing authority, rebuilds the UKIs and verifies their
Secure Boot signatures before selecting the default. It preserves TPM
enrollment and recovery credentials. CachyOS, stock Linux, its fallback UKI and
Linux 6.12 LTS remain available as signed boot entries.

`bootctl --variables=no list` shows the current on-disk entries. Plain
`bootctl list` can retain removed entries as `reported/absent` from the volatile
EFI inventory until the next normal boot refreshes it.

### Predator authenticated boot

Lanzaboote verifies signed boot images and measures their kernel, initramfs and
command line into SHA-256 PCR 11. All five disks require post-separator SHA-256
PCR 7 plus signed PCR 11: root at `enter-initrd`, data at
`enter-initrd:leave-initrd`, with separate signing authorities. `sysinit` waits
for the bounded data-unlock attempts before closing their authorization window.

Normal NixOS rebuilds automatically provision recovery, sign image companions
and restore the previous default if installation fails. The public companions
carry signatures into initrd without embedding disk keys. Updates need no disk
re-enrollment. Five boot generations are retained; deleting old files does not
revoke previously issued image signatures.

Preserve `/var/lib/predator-boot` and `/var/lib/sbctl` during reinstallation.
Private keys stay on encrypted root and in the encrypted recovery backup,
outside Git and the Nix store. LUKS headers contain TPM-sealed unlock credentials;
mounted volume keys remain in RAM. After a normal reboot, run
`sudo predator-boot audit` to verify unlocks and late-phase rejection.

#### Recovery

The console accepts the LUKS recovery passphrase even without a working TPM.
Initrd SSH on `192.168.1.19:2223` uses a TPM-sealed identity released only in the
authorized initrd phase. Only ciphertext enters the ESP; the runtime key and SSH
sessions are removed at switch-root. An unseal failure leaves console recovery
available.

Main and laptop keys are authorized; SSH password login is disabled. Connect
using the pin in `hosts/predator/recovery-known-hosts`:

```sh
ssh -o StrictHostKeyChecking=yes \
  -o GlobalKnownHostsFile=/dev/null \
  -o UserKnownHostsFile="$HOME/dev/dotfiles/hosts/predator/recovery-known-hosts" \
  -p 2223 root@192.168.1.19
```

Then run `systemd-tty-ask-password-agent --query` to answer the unlock prompt.
Secure Boot key or PCR-bank changes may require fresh disk enrollment and
recovery-credential sealing.

The TPM-free `pm-study` specialisation uses passphrase recovery and manual data
unlocks. Its suspend-study hold and scheduling controls remain separate from
normal authenticated boot.

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

Use Git-backed flake references such as `.` or `.#host-tools` to exclude
`.agents-local/`. Local edits are included; new source files need `git add`, not
a commit. Avoid `path:`, which copies ignored build caches before source filters
run and can race with builds modifying them.

```sh
nix flake check . --no-build --no-update-lock-file
```

This evaluates all NixOS and Arch Home Manager profiles and checks host isolation,
service lifecycle rules, and Predator's boot/storage invariants. Run the executable
unlock-generator and Arch maintenance tests with:

```sh
nix build .#checks.x86_64-linux.data-unlock .#checks.x86_64-linux.arch-maintenance --no-link
```

These tests use generated units and disposable files; they do not activate a system.

## OpenCode extensions

The bootstrap seeds OpenCode 2.0.28 and preserves existing v2 installs so T3 can
update them. Its service runs `~/.local/bin/opencode` on Arch and NixOS.
OpenChamber 2.3.1 stays managed by Arch packages or this flake on NixOS; remove
the old manual `IgnorePkg = openchamber` pin when upgrading it.
The native service generates its private password in
`~/.config/opencode/service.json` and uses the fixed Basic username `opencode`.
OpenChamber receives that credential at startup; the idle watchdog reads it.
The old SOPS API password is no longer loaded by these services.

At activation, stop the v1 server and restart `opencode.service` after installing
the prepared configuration. Rebuilds intentionally keep a running server; this
major upgrade needs an explicit restart. `serve --service` registers this daemon
for terminal discovery and keeps one queue owner.
The Nushell `oc`/`occ` shortcuts use that discovery; `opencode-plugin-update`
uses the native package updater.
Codex's Composio headers helper and OpenCode's auth plugin reuse the existing
Composio CLI login; neither needs a separate OAuth login.

Chezmoi owns native `opencode.jsonc` and global `cli.json`. The server config
uses `plugins`, native MCP/permission/provider shapes, and Sleev provider
headers beside `settings.baseURL`. Sleev's v2 setup omits `compaction.prune`;
automatic compaction keeps its normal default. Terminal plugins use absolute
file URLs because native CLI plugin paths resolve from the working directory.
Runtime TUI preference changes write to `cli.json`; save them in its chezmoi
source too. Native keybindings apply apart from Ctrl-Alt-W for workspaces:
Ctrl-O opens recent sessions/projects, Ctrl-Alt-U/D scroll half a page, and
F2/Shift-F2 cycle recent models.

The local browser/computer-use worktrees prepare 2.0.0 for OpenCode v2 and
separate 1.0.0 releases for v1. Their Rust MCP binaries and Codex manifests are
shared across harnesses. These prepared configs use local packages until npm
publication; applying them is a separate activation step.

The retained extensions are Composio auth, Git diff handling, a small subagent
model-selection policy. The policy lets
agents choose available models and variants, preferring lightweight workers for
routine tasks and reserving Astra for explicit requests. Native delegation owns
child validation, nesting limits, completion notifications and model changes.
The old forced routing, `/subagents` picker, saved modes and model pinning are
retired. The shared orchestration skill still describes the delegation workflow.
Commands inherit the selected model. Native recovery replaces custom retry/subagent
watchdog extensions; native terminal theme detection selects Rose Pine's light
or dark palette. All native model variants remain available. The separate Rust
idle watchdog uses the v2 server API.

`pkgs/opencode-extensions` owns the local server plugins and their tests.
Its Nix build bundles dependencies with a locked toolchain. Home Manager installs the compiled files; chezmoi owns the JSON
settings. Build and test the extensions and the compiled idle watchdog with:

```sh
nix build .#opencode-extensions .#opencode-idle-watchdog --no-link
```

On Arch, activate Home Manager before applying updated TUI settings. On NixOS,
rebuild the system instead. Restart OpenCode to load the compiled plugins. The
watchdog keeps the `opencode-idle-watchdog` command and no longer compiles itself
on invocation.

## Agent prompts

OpenCode's skills are the canonical prompts: `~/.config/opencode/skills/*/SKILL.md`,
tracked under `dotfiles/private_dot_config/opencode/skills/`. Edit the live skill
files and save them with `chezmoi re-add` after reviewing the shared repository.

Home Manager links `~/.agents/skills` to the editable OpenCode skills for Codex
and Hermes. [Codex discovers user skills through that directory](https://developers.openai.com/codex/skills).
Changes are visible through the link without copying files or running a sync
command. Restart agents when their loaded skill lists need refreshing.

The old commands have migrated to skills; `commitdiff` and `autocommit` are
retired. Home Manager owns the shared link, and chezmoi ignores it. Codex's
built-in skills remain managed by Codex.

## REA reverse engineering

The workstation Home Manager profiles install [REA](https://rea.tools/) and
Ghidra on Arch and NixOS. `pkgs/rea` pins the published CLI/MCP runtime and its
dependencies. Its wrappers select Nix-provided Node 24, Ghidra 12.1.x and JDK 21,
so OpenCode needs no shell environment setup or runtime package downloads.
The host-aware chezmoi `opencode.jsonc` template owns the local model provider
and disabled `rea` MCP registration; Home Manager does not generate
`opencode.json`. Home Manager installs the matching `reverse-engineer-anything`
skill and references.

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
the policy authorities under `/var/lib/predator-boot` when reinstalling.
`disko.nix` is only for a fresh installation and formats the SSD and all three HDDs.

SMART checks all five physical disks, runs daily short self-tests at noon and
weekly long tests on Sunday: SSDs and WD at 13:00, Seagate at 14:00 and Toshiba
at 15:00. Temperature emails start at 55 C for HDDs and 80 C for SSDs.
SMART sends warnings through Resend to
`mirsella@protonmail.com`. USB disks can reconnect
without stopping monitoring of the other disks. ZED emails device faults,
I/O errors, checksum errors and scrub failures using the same Resend key as
Nextcloud. The kernel-journal watcher also sends USB connection errors to Telegram.
Both pools receive a monthly scrub; Sanoid retains daily and weekly snapshots.
Snapshots and database dumps on `tank` do not survive loss of that pool.

A nightly restic backup copies `fast/data` and `fast/ncdata` from throwaway
`.zfs/snapshot/restic-backup` snapshots to `/srv/backup/restic` nightly at
23:50, keeping 7 daily, 8 weekly and 6 monthly snapshots; a monthly check reads
10% of the repository data. Every scheduled backup job pings a healthchecks.io
check on success. The overnight idle check waits for the backup jobs to finish.

`recovery-backup.service` runs on Sundays at 23:10 and retains 7 root-only
bundles in the separate `tank/backup/recovery` dataset at `/srv/backup/recovery/`; `latest`
points to the newest complete bundle. Each
contains all five current LUKS headers and UUIDs plus `system-secrets.tar` with
`/var/lib/predator-boot`, the SOPS SSH host identity and `/var/lib/sbctl`.
These archives contain private keys. Restore them as root with their original permissions.

`db-backup.service` uses the same private dataset, publishing database dumps and
Nextcloud configuration under `/srv/backup/recovery/db/latest` at 23:00. Both
backup jobs stage complete bundles, atomically publish `latest`, and retain 7
bundles.

`fast/tank-pre-raidz1-20261003` holds a read-only, unmounted migration copy,
including the older snapshots. It is a fixed recovery point and does not track
later writes.

SMART lifetime counters cannot generally be cleared. The post-cable baseline and
migration snapshot manifest are saved under
`/var/lib/storage-baselines/post-cable-20261003/` on Predator. Compare future
readings with these values to distinguish new errors from the drive's history.

## Predator overnight suspend

The timer checks every minute from 00:30 through 06:59 local time. It requires
30 minutes without authenticated application or completed T3 turn activity,
no logged-in SSH/local users, and no active maintenance or storage blocker. It
warms the USB disks, rechecks user activity, sets a 07:00 RTC alarm and requests
deep suspend with systemd inhibitors enabled.

`pkgs/host-tools/src/activity.rs` recognizes Nextcloud's response-side
`X-User-Id` header and successful protected API requests for Immich, OpenCode
and OpenChamber. Valid API keys and authorized shared-photo access also count.
Public pages, health checks, failed logins, scanner requests and unauthenticated
SSH connections do not reset the quiet period. Keeping an app logged in without
requests does not count as activity by itself.

Forwarding-only SSH connections, including T3's persistent environment tunnel,
do not block suspend. Interactive SSH sessions and connections running commands,
SFTP or other transfers still block it. Logind sessions in `closing` state after
logout do not count as open logins, even if a detached server keeps their scope
alive. The guard reads T3's local
`~/.t3/userdata/statev2.sqlite` database read-only: unfinished turns, executions,
tools, background tasks and queued effects block sleep. Idle provider processes
and held turn queues do not. Missing, corrupt or incompatible T3 state fails the
check and prevents automatic suspend; the journal records the error.

This T3 guard applies to the overnight timer, not manual suspend. For detached
jobs or commands outside T3's tracked work, explicitly hold a systemd inhibitor:
`systemd-inhibit --what=sleep --mode=block --why="Remote job" your-command`.
Arbitrary traffic through an SSH tunnel is not itself an activity signal.

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
sudo host-tools night-suspend --activity-only --t3-state /home/mirsella/.t3/userdata/statev2.sqlite
CARGO_TARGET_DIR="$PWD/.agents-local/host-tools-target" cargo test --locked --manifest-path pkgs/host-tools/Cargo.toml
```

The protected API route lists need review when upgrading these applications or
changing Caddy's hostnames and upstreams.

## Custom service programs

`pkgs/host-tools` contains the Rust implementations of our recurring host jobs.
Nix and systemd define their schedules, dependencies, users and permissions.
The subcommands cover overnight suspend, storage alerts and mail, database and
recovery backups, Beszel and Nextcloud setup, lid/backlight handling, Ryzen power
limits, Bluetooth audio profiles, Btrfs headroom checks, Wake-on-LAN and netconsole.

Build the Nix package with `nix build .#host-tools`. A native build for Arch
or Debian is also supported:

```sh
export CARGO_TARGET_DIR="$PWD/.agents-local/host-tools-target"
cargo build --release --locked --manifest-path pkgs/host-tools/Cargo.toml
sudo HOST_TOOLS_BINARY="$CARGO_TARGET_DIR/release/host-tools" python3 arch/maintenance/apply.py
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
`config.toml` and `modes.json` from `dotfiles/system/main/etc/coolercontrol/` as
writable root-owned files in `/etc/coolercontrol/` before starting the daemon.
Systemd creates the configuration directory. Changes to either file change the
service unit, so `nixos-rebuild switch` stops the old daemon before installing them.

Unrelated rebuilds leave the running daemon and its config alone. GUI changes
last until the daemon restarts; copy profile and mode edits back into the tracked
files to keep them. CoolerControl maps power saver to Quiet, balanced to Balanced,
and performance to Performance for all six motherboard fan channels, using CPU
temperature. It interpolates between curve points:

- Quiet holds 20% through 60°C, reaches 30% at 70°C and 50% at 85°C, and caps at
  60% from 90°C. This mode trades cooling headroom for less noise under load.
- Balanced holds 20% through 50°C, reaches 40% at 60°C and 70% at 70°C, then
  reaches 100% at 85°C.
- Performance holds 20% through 45°C, reaches 45% at 55°C and 75% at 65°C, then
  reaches 100% at 80°C.

On Arch, stop `coolercontrold`, install the tracked files into `/etc/coolercontrol/`,
then start it again; Home Manager does not manage the system daemon.
