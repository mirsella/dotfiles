# Machine configuration

The flake discovers host directories under `hosts/` and uses each directory name
as the hostname. NixOS and standalone Arch Home Manager profiles share the same
workstation home configuration.

Every host directory exports a complete `nixosModules.<hostname>` and
`nixosConfigurations.<hostname>`. Its hardware file is required; `home.nix` also
exposes the standalone Arch `homeConfigurations.<hostname>`.

| Host | NixOS role | Machine-specific settings |
| --- | --- | --- |
| `main` | Plasma desktop | Rust CPU/GPU fan controller and NCT6798 driver |
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

Predator's checkout is owned by `mirsella`, and root cannot evaluate a Git flake
owned by another user (`repository path ... is not owned by current user`), so
`sudo nixos-rebuild` fails there. Build as the owner and activate as root:

```sh
toplevel=$(nix build '/home/mirsella/dev/dotfiles#nixosConfigurations.predator.config.system.build.toplevel' --print-out-paths --no-link)
sudo nix-env -p /nix/var/nix/profiles/system --set "$toplevel"
sudo "$toplevel/bin/switch-to-configuration" switch
```

For the current Arch installations, use `homeConfigurations.main` or
`homeConfigurations.laptop` with standalone Home Manager. Pacman/AUR still own
their application binaries, except Hex and the writable agent installs below. NixOS
supplies other binaries from Nix packages.
Chezmoi continues to own editable application dotfiles; NixOS installation and
fan configuration do not run chezmoi hooks.

The workstation Home Manager profiles mask `akonadi_control.service`, including
starts requested through D-Bus. KMail, KOrganizer and other KDE PIM apps are
unused, so their Akonadi database stays off even if a package installs it again.
Remove the mask from `modules/home/workstation.nix` before using those apps.

### CodexBar

Main and laptop share the OpenCode Go key in `secrets/codexbar.yaml`, encrypted
with SOPS for the existing age recipients. Home Manager decrypts it into
`~/.config/codexbar/opencode.env` with mode `0600`. The managed
`~/.local/bin/codexbar` launcher loads that file only for CodexBar. Chezmoi owns
the provider settings in `~/.config/codexbar/config.json`; keep API keys out of
that JSON file.

NixOS installs the pinned CLI and Plasma widget through the workstation profile.
Home Manager installs the widget on Arch too, carrying the local all-window pace
patch until upstream releases it. On Arch, install the CLI natively before
activating Home Manager:

```sh
paru -S --needed codexbar-cli
home_profile=$(nix build ".#homeConfigurations.$(hostname).activationPackage" --no-link --print-out-paths)
"$home_profile/activate"
chezmoi apply ~/.config/codexbar/config.json
codexbar usage --provider opencodego --source api --format json
```

After pulling dotfiles updates on another workstation, repeat the activation
and chezmoi apply. Home Manager reloads the secret automatically. The key needs
no browser login there. Refresh the Plasma widget after changing its credentials. Codex still
uses each machine's own local login.

### OpenCode, Codex, Claude Code and T3 Code

All three hosts import `modules/home/agent-tools.nix`. Home Manager runs the
Nix-built `bootstrap-agent-tools` helper during activation. It seeds missing
official standalone installs: OpenCode v2 CLI, Codex CLI (latest stable), Claude
Code CLI (pinned seed), T3 CLI (nightly), and a separate nightly T3 desktop
AppImage. Existing installs are left untouched. To apply just this bootstrap
without switching the rest of a host's configuration:

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
`~/.t3/runtime/versions`. Claude Code uses Anthropic's native layout:
`~/.local/bin/claude` links into `~/.local/share/claude/versions`. The desktop
AppImage is a writable file at
`~/.local/share/t3code/app/T3-Code-nightly.AppImage`. A Home Manager or NixOS
switch never replaces or downgrades an existing native install, even when its
version, channel, or major release differs from the pinned seed; it only seeds
a missing or dangling install. `opencode upgrade`, `codex update`,
`claude update`, T3 Code, and the desktop's updater own subsequent releases.
Claude Code also auto-updates in the background, and T3's Claude provider runs
`claude update` itself. The initial CLI and desktop share a pinned nightly
version; updating that seed only affects fresh installs.

Nix supplies the `t3code`/`t3code-nightly` launchers and desktop menu entry. On
NixOS the launcher uses `appimage-run`; Arch runs the AppImage directly. A service
drop-in puts `~/.local/bin` first in T3's PATH while preserving existing shell
exports without overriding T3's temporary directory. Bootstrap installers also
use their default temporary directories. T3 owns its
background-service unit and updater state. Bootstrap installs the T3 service
when a user manager is available; otherwise the next Home Manager activation
retries. `t3 update` repoints T3's tracked CLI launchers and restarts the
service; desktop-app updates do not update `~/.local/bin/t3`. `t3-settings`
runs the running server's own runtime CLI, so a stale launcher does not break
capture or sync. Home Manager runs no settings watcher and no CLI-alignment
unit.

Chezmoi stores portable T3 preferences and project registrations in
`~/.config/t3code/portable-settings.json`. The Nix-built `t3-settings` helper
captures fields listed in `pkgs/host-tools/src/t3-settings-allowlist.json`.
It excludes credentials, provider environment variables, browser profiles,
saved connections, onboarding state and window geometry. Raw `~/.t3` and
Electron profile directories are ignored by chezmoi. Conversations and
attachments need a separate backup.
T3's OpenCode provider omits `serverUrl` so T3 launches its own server
and injects its thread-scoped MCP tools. The shared standalone service on
port 14096 was retired and OpenChamber manages its own OpenCode. If an instance
uses a local service, sync reads its password from `~/.config/opencode/service.json`
into local T3 settings; the next `chezmoi apply` refreshes it after rotation.
The password stays out of portable captures and Git.
Other endpoints keep their own credentials. GitHub device authorization and
agent-provider logins remain device-local.
Capture and apply report unknown field paths without printing their values.
Add a field to the allowlist to capture it, or set its rule to `false` to
explicitly ignore it. Known exclusions do not produce warnings.

Capture writes a version 2 snapshot of all allowlisted persisted values,
including explicit `false` and `null`. Apply resets omitted managed keys to
the destination T3 server's defaults. This also syncs switches that T3 turns
off by deleting their saved value, rather than leaving an older machine's
enabled setting untouched. Unknown fields and device-local exclusions stay
intact. Keybindings and other lists are replaced in full.

Snapshots do not need a desktop AppImage or extracted defaults, so capture and
sync work on headless hosts and while the desktop and server versions differ.
The helper still requires its runtime CLI to match the running server.
Legacy version 1 bundles retain their sparse-merge behavior and default-pruning
version guard; `--allow-version-mismatch` bypasses that guard without pruning,
and `apply --remove-absent` explicitly resets their omitted managed keys.
An existing pruned v1 baseline may need `sync --force-apply` or
`sync --force-capture` once when adopting v2; the helper does not guess which
previously omitted values were defaults.

Preferences changed in the app are captured into the portable bundle when
`chezmoi re-add` runs. Repository changes reach T3 when `chezmoi apply` (or
`chezmoi update`) runs its post-hook: `t3-settings sync --if-running
--no-capture` applies bundle changes but never writes live-only changes back
into the chezmoi-managed bundle. When both sides changed, sync stops with both
diffs and leaves settings unchanged. Resolve a conflict with
`t3-settings sync --force-capture` (live wins) or `--force-apply` (bundle wins);
after a live-wins capture, run `chezmoi re-add` to save it to the source. The
hook skips when no T3 server is running and passes `--dry-run` when chezmoi is
previewing. There is no background watcher. Every overwrite, adoption, and
conflict is archived with before/after snapshots under
`~/.local/state/agent-tools/t3-settings-history/` for recovery. If apply reports
live-only changes, run `re-add` to save them.

To capture and commit the bundle to Git:

```sh
chezmoi re-add ~/.config/t3code/portable-settings.json
git diff -- dotfiles/private_dot_config/t3code/private_portable-settings.json
```

The `re-add` pre-hook runs `t3-settings capture --if-running` to refresh the
local portable bundle before chezmoi copies it into the source. Captures
attempted with no live T3 server skip with a diagnostic; capture errors stop
`re-add`. Its post-hook records the sync baseline only when the source copy
matches the captured bundle. A dry run, failed re-add, or re-add of an unrelated
file does not acknowledge settings that were never saved into the repository.
The hook also runs for targeted and dry-run invocations: it refreshes the local
bundle, while chezmoi controls which files are saved. To save only T3 settings,
use `chezmoi re-add ~/.config/t3code/portable-settings.json`.
The `apply` and `update` post-hooks run
`t3-settings sync --if-running --no-capture` after chezmoi writes managed files,
so a normal `chezmoi apply` or `chezmoi update`
applies repository changes without modifying the portable bundle. Live-only
changes are reported and left for `chezmoi re-add`; a conflict stops the hook
and leaves both sides unchanged. Resolve it with the force flags, then run
`re-add` if live settings won. Chezmoi's usual automatic commit and push apply.
Review other repository changes before `re-add`; automatic commits can include
unrelated staged changes, and the captured bundle includes project script
commands and URLs. On each receiving machine, `chezmoi update` must first pull
the repository successfully; it does not resolve Git conflicts or divergent work
for you. `chezmoi update --apply=false` only pulls and does not sync settings.
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

Run `chezmoi init` to regenerate the hooks in `~/.config/chezmoi/chezmoi.toml`
on an existing machine. That config is generated from the source template, not
managed as a normal destination file.

On another machine, clone the project checkouts, start the T3 server, then:

```sh
chezmoi apply ~/.config/t3code/portable-settings.json
t3-settings apply --dry-run
t3-settings apply
```

Apply merges allowed fields and preserves local credentials and unlisted
settings outside lists. List settings, including keybindings, favorites,
model options and project scripts, are replaced in full; unknown fields
inside their items are not preserved. Version 2 snapshots reset omitted managed
preferences to T3 defaults; legacy version 1 bundles leave them unchanged unless
`--remove-absent` is given. Project paths under the source user's home become
`~/...`; registration uses T3's
project API and maps overrides to the destination IDs. Project settings use
T3's canonical overrides; derived legacy views stay local. Missing checkouts
are skipped with a warning; clone them and apply again. Existing conversations and
projects outside the bundle are preserved. Saved server exposure and local
environment preferences apply too. Close the settings editor while restoring,
then restart the desktop to reload client preferences. Neither command clones
repositories or restores authentication. Connections must be paired locally.

T3 Connect's mobile activity publishing is account-linked state, not a portable
setting. On each signed-in host, use `t3 connect publish` to enable push
notifications and Live Activities, and `t3 connect status` to verify it. Keep
Connect credentials and device pairings out of the bundle.

### Hex voice dictation

`modules/home/hex.nix` starts Hex with the graphical session on main and laptop.
Home Manager owns `hex.service` and a hidden XDG autostart entry so the app's
Launch at login toggle cannot create a second login launcher. `hex-model.service`
installs the selected model if needed. Nix compiles Hex from the upstream `dev`
branch with upstream's own `nix/package.nix`, through the `hex` flake input, on
both Arch and NixOS, including matching Mesa Vulkan drivers. The CLI and desktop
launcher use that same Nix package. `nix flake update` advances the input;
activate Home Manager after updating it to install a newer commit. On Arch,
restart `hex.service` after activation to use the new build immediately. Hex
does not replace the Nix-owned executable itself.

Chezmoi tracks `~/.config/hex/portable-settings.json`, captured from the current
preferences with `hex-settings capture`. The bundle selects Cohere Transcribe,
English transcription and UI, Meta+Space dictation, Ctrl+Alt+V paste-last,
toggle recording, the current Labs switches and five-minute idle unloading.
T3's portable settings also select Cohere and English. Both applications use the
same checksum-verified Q8_0 model: Home Manager links T3's model cache entry to
Hex's downloaded file. There is one 2.41 GB download per machine, outside Git and
the Nix store. Loading it in both apps can still use separate RAM/GPU memory.
The allowlist in `pkgs/host-tools/src/hex-settings-allowlist.json` includes
shortcuts, output modes, rewrite prompts and vocabulary. Review personal text
in those preferences before saving it to Git.

Microphone names, local folders, keyboard input-source IDs, model recents,
calendar identifiers, capture routing and migration state stay on each device.
Credentials, portal tokens, recordings, history and model downloads never enter
the bundle. Do not add `~/.local/share/hex` to chezmoi.

`chezmoi re-add` captures preferences before saving the portable file. Apply
merges bundle changes without replacing private fields; Hex reloads the file
while idle. Local-only edits remain untouched until capture. Conflicting edits
stop sync: choose `hex-settings capture` for live settings or
`hex-settings sync --force-apply` for the bundle, then re-add after capture.
The helper keeps a baseline and the last overwritten settings file, mode 0600,
under `~/.local/state/hex-settings`. It has no background watcher.

```sh
chezmoi re-add ~/.config/hex/portable-settings.json
hex-settings sync --dry-run
```

Chezmoi auto-commits and pushes; review unrelated staged changes before re-add.
On a new workstation, apply the portable file and chezmoi hook configuration,
then activate Home Manager. Portal approvals and microphone selection remain
device-local.

### Workstation memory

Main and laptop share `modules/workstation-memory.nix` on NixOS. It sets zstd
zram to twice physical RAM in logical capacity, with priority 100, swappiness
150 and watermark scale 125. Zram allocates physical RAM as needed; the logical
limit does not reserve that amount. Predator keeps its existing server defaults.

On Arch, generate and install the same policy without switching Home Manager:

```sh
mkdir -p ~/.local/state/workstation-memory
nix build .#workstation-memory-config --out-link ~/.local/state/workstation-memory/config
sudo install -Dm644 ~/.local/state/workstation-memory/config/etc/systemd/zram-generator.conf /etc/systemd/zram-generator.conf
sudo install -Dm644 ~/.local/state/workstation-memory/config/etc/sysctl.d/99-swappiness.conf /etc/sysctl.d/99-swappiness.conf
sudo sysctl --load=/etc/sysctl.d/99-swappiness.conf
```

Sysctl changes apply immediately. Reboot to apply a changed zram capacity;
resetting an active device would require moving its swapped pages elsewhere.
Files under `dotfiles/system/main/` are captured system backups, not the policy
source.

### Bootvine boot display

All NixOS host profiles enable the pinned upstream Bootvine module for the
initrd, normal boot and shutdown. Rebuild the system profile to apply it.
Predator keeps its existing console log level for suspend debugging, and its
TPM policies and recovery SSH are unchanged.

On main and laptop running Arch, pacman owns Bootvine and its system units:

```sh
sudo pacman -S --needed bootvine
sudo systemctl enable bootvine.service bootvine-shutdown.service
```

Do not start the units manually: they take a virtual terminal. Enabling them
prepares the next boot and shutdown without interrupting the desktop.

Laptop's generated `arch/laptop-boot.nix` configuration adds `sd-bootvine` to
its existing systemd initramfs. After installing the package, apply and verify
the signed UKIs with the refresh command below. Main retains its udev-based
initramfs, so Bootvine starts after switch-root there. Neither machine needs
changes to its kernel command line or disk-unlock policy.

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
Timeshift snapshot plus two pre-upgrade snapshots, a 1 GiB journal cap, old Docker
builder cache pruning, and limited Btrfs maintenance. When `fail2ban` is installed, the
installer also enables its SSH jail: three failed logins ban an address for a
minute, then five, ten, twenty minutes for repeat offenders, doubling up to a
week, and the LAN is trusted. Arch
uses btrfs-progs' monthly scrub timer and its own balance and space-check units.
The desktop Home Manager profile caps the kache build cache at 150 GiB. NixOS
uses the equivalent shared Nix GC, journal and fail2ban settings, and the
desktops trim their encrypted SSDs weekly.

On Arch, `modules/home/arch-package-cache.nix` configures the native weekly
`paccache.timer` to keep the newest cached version of each package. Home Manager
activation requires `pacman-contrib` and passwordless sudo to install its root
service drop-in and enable the timer on main and laptop. It does not uninstall
packages or prune Paru's build directories. Run `sudo systemctl start
paccache.service` to clean the cache immediately; older cached versions will no
longer be available for offline downgrades.

On Arch, Home Manager also prunes the user's Nix profiles daily with the same
7-day retention; the root timer cannot prune profiles in the user's XDG state
directory. Active profiles and their dependencies stay rooted, including main's
GGUF model. Builds can also trigger GC below 10 GiB free, targeting 20 GiB free.

## Android wireless debugging

`adb-wireless.service` starts at login on the workstations. It subscribes to ADB's
device-change notifications and reconnects each configured phone (`188b31ee` on
every workstation, plus the FP3 `A20AAFYS0202` on the laptop) using its current
IP and port from mDNS. While connected it re-checks mDNS every five seconds and
reads the device socket's local payload counters every minute, but otherwise
waits without running anything else. A disconnect wakes it immediately; while
disconnected it retries discovery every two seconds through ADB's local server at
`127.0.0.1:5037`.
ADB's native auto-connect handles new announcements. The service starts the server
if it exits and logs state changes and connection errors to the user journal.
It waits up to 90 seconds for a default route before starting: the ADB server
binds its mDNS responder to the interfaces that exist when it starts, so a login
that beats the Wi-Fi lease would otherwise leave discovery dead until restart.
ADB enables `SO_KEEPALIVE` with a one-second idle and probe interval on each
device socket, so each connected phone is woken once a second while a transport
is open. The watcher holds that connection only while it is wanted. It drops a
transport once its phone has stopped advertising for fifteen seconds, which
keeps an mDNS cache expiry or a Wi-Fi roam from cutting a live connection.

The workstations share the phone's idle shutdown. When a sample shows payload
traffic, the watcher touches `/data/local/tmp/adb-wireless/<hostname>` on the
phone, at most every 30 minutes while the phone stays in use, and leaves that
write out of its own accounting. Keepalive probes carry no payload, so they never
count as use; the counters come from `ss`, which is why the unit's PATH carries
`iproute2`. Once a watcher has seen no traffic for three hours
(`services.adb-wireless.idleMinutes`), it reads the other workstations' marker
ages on the phone's clock. A marker younger than the limit plus those 30 minutes
keeps Wireless debugging on until it ages out; otherwise the watcher runs
`settings put global adb_wifi_enabled 0`, which turns Wireless debugging off
and ends every connection. A host that is offline or whose watcher stopped
simply leaves its marker ageing. Use from a host whose watcher is not running is
not recorded. Shutdown comes three to four hours after the last use from any
workstation. While all hosts are idle the phone sees only one marker
read at each deadline. If the markers cannot be read or the shutdown fails,
Wireless debugging stays on and the watcher asks again after 15 minutes.
Enabling Wireless debugging again on the phone itself, from a Quick Settings tile
or Developer options, makes it advertise and the watchers reconnect within
seconds.

On Arch it uses Android Studio's SDK at
`~/.local/share/android/sdk/platform-tools/adb`; NixOS uses `android-tools`.
Pair each workstation once through Android's **Wireless debugging → Pair device
with pairing code**, then run `adb pair IP:PAIRING_PORT`. Keep Wireless debugging
enabled and the phone on a LAN where mDNS reaches the workstation. If Android
turns Wireless debugging off, enable it again on the phone. The service cannot
turn it on remotely after losing ADB access.

After applying Home Manager on Arch, start the service for the current login:

```sh
systemctl --user start adb-wireless.service
journalctl --user -u adb-wireless.service -f
adb devices -l
```

Stop `adb-wireless.service` before intentionally disconnecting a watched phone,
otherwise it reconnects when ADB reports the disconnect. Watched serials default
in `modules/home/adb-wireless.nix`; a host adds phones it is paired with, as the
laptop does for the FP3.

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
nix eval .#ci.drvPath --raw --no-update-lock-file
nix flake check . --no-build --no-update-lock-file
```

The first command materializes Hermes's filtered sources without building the
CI packages. It is needed on a cold store or after garbage collection because
`--no-build` makes the flake check read-only, while Hermes reads metadata from
those generated source paths during evaluation.

This evaluates all NixOS and Arch Home Manager profiles and checks host isolation,
service lifecycle rules, and Predator's boot/storage invariants. Run the executable
unlock-generator and Arch maintenance tests with:

```sh
nix build .#checks.x86_64-linux.data-unlock .#checks.x86_64-linux.arch-maintenance --no-link
```

These tests use generated units and disposable files; they do not activate a system.

## OpenCode extensions

The bootstrap seeds OpenCode 2.1.4 and preserves every existing standalone
install, including newer major versions, so T3 and the CLI updaters own them.
Its service runs `~/.local/bin/opencode` on Arch and NixOS.
OpenChamber 2.5.2 stays managed by Arch packages or this flake on NixOS; remove
the old manual `IgnorePkg = openchamber` pin when upgrading it.
The native service records its runtime URL, PID and generated password in
`~/.local/state/opencode/service.json` and uses the fixed Basic username
`opencode`; `~/.config/opencode/service.json` remains a fallback.
OpenChamber runs its own managed `opencode serve`. The idle watchdog reads each
server's credential from its environment or the native service state and
monitors only `opencode serve` processes that expose the v2 API. The old SOPS
API password entry was removed.

The shared `opencode.service` on port 14096 is retired: OpenChamber no longer
attaches to it (`UnsetEnvironment` drops the old `OPENCODE_HOST` pins) and the
CLI and T3 use their own or the lazily started background service.
The Nushell `oc`/`occ` shortcuts use that discovery; `opencode-plugin-update`
uses the native package updater.
Codex's Composio headers helper and OpenCode's auth plugin reuse the existing
Composio CLI login; neither needs a separate OAuth login.

Chezmoi owns native `opencode.jsonc` and global `cli.json`. The server config
uses `plugins`, native MCP/permission/provider shapes, and Sleev provider
headers beside `settings.baseURL` for `openai` and `opencode-go`. The `opencode`
provider connects directly because its free tier rejects gateway traffic.
OpenCode's official `claude-agent` provider uses the Claude Code login, so it
follows the Claude Code Sleev proxy. Run `sleev harness claude-code enable` and
connect `claude-agent` in OpenCode on each machine. Sleev's v2 setup omits
`compaction.prune`;
automatic compaction keeps its normal default. Terminal plugins use absolute
file URLs because native CLI plugin paths resolve from the working directory.
Runtime TUI preference changes write to `cli.json`; save them in its chezmoi
source too. Native keybindings apply apart from Ctrl-Alt-W for workspaces:
Ctrl-O opens recent sessions/projects, Ctrl-Alt-U/D scroll half a page, and
F2/Shift-F2 cycle recent models.

OpenCode v2 installs Browser MCP and Computer Use MCP from their published npm
packages, and Codex installs the same plugins from their Git marketplaces at
`release/latest`. A fresh setup needs no development checkout, compiler or Nix
package to start either MCP; each harness populates its own plugin cache. Home
Manager selects the Chromium executable through `environment.d/50-opencode.conf`.

OpenCode loads the published plugins directly and Queue from `opencode-queue@2.0.2`.
Context7 Broker remains pinned to its published 0.4.0 release. Each release
refreshes the Codex marketplace branches; install or update the two plugins with:

```bash
codex plugin marketplace upgrade
codex plugin add browser@browser-mcp
codex plugin add computer-use@computer-use-mcp
```

These commands populate Codex's writable plugin cache; enabling a plugin in
`config.toml` alone does not install it. Start a new Codex session afterward.

Both MCPs start their own backends on demand. Computer Use's background desktop
starts a private Wayland compositor, D-Bus, PipeWire, portals and AT-SPI from its
package closure, using Mesa software rendering for the private compositor.
Foreground control uses the logged-in Plasma Wayland session;
run `computer-use-mcp init` there to approve portal access. `computer-use-mcp doctor`
checks prerequisites without requesting access or proving capture and input.

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
idle watchdog uses the v2 server API, ignores TUI and `--standalone` clients, and
scopes pending forms and permission requests by `location[directory]`. Its
terminal dashboard keeps the status header fixed and scrolls a long session list
with the arrow keys or `j`/`k`, PgUp/PgDn, and Home/End; `q` exits. It lists
running and waiting sessions plus sessions that were active earlier in the
current watch, so old idle history does not fill the table.

`pkgs/opencode-extensions` owns the local server plugins and their tests.
Its Nix build bundles dependencies with a locked toolchain. Home Manager installs the compiled files; chezmoi owns the JSON
settings. Build and test the extensions and the compiled idle watchdogs with:

```sh
nix build .#opencode-extensions .#idle-watchdog --no-link
```

On Arch, activate Home Manager before applying updated TUI settings. On NixOS,
rebuild the system instead. Restart OpenCode to load the compiled plugins. The
shared `pkgs/idle-watchdog` Cargo project builds both watchdog commands and no
longer compiles itself on invocation.

The `opencode-idle-watchdog` command watches OpenCode's v2 server API. The
`t3-idle-watchdog` command in the same package waits on T3 Code's local orchestration shell
API. It discovers the running `t3 serve` process and active CLI, issues a
read-only bearer token with a configurable TTL (12h by default), and excludes a
thread with `--ignore-thread` or a `T3CODE_THREAD_ID`/`T3_THREAD_ID` supplied by
the launcher. Issued tokens are cached with mode 0600 at
`~/.cache/t3-idle-watchdog/token.json` and reused until five minutes before
expiry, so restarts avoid the T3 CLI; delete that file to force a fresh token.
Tokens are not revoked on normal exit and expire with their TTL. T3 does not
currently set a thread ID for shell commands, so pass
`--ignore-thread <thread-id>` when launching it from inside a T3 thread. Pending
questions, permissions and thread waits are shown as waiting without blocking
idle; queued, running and background work block. Pull-request watch monitors are
treated as external waits, so they do not keep the watchdog from suspending. Use
`--once` to print the current classification without waiting; it exits 0 when
idle, 2 when blocked, and 1 on errors. By default an unavailable API blocks
forever; `--stuck-after SECONDS` drops only transport failures after that delay;
all other errors remain blocking. Home Manager installs
both watchdog commands under `~/.local/bin`.

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

## leanrea reverse engineering

The workstation Home Manager profiles install
[leanrea](https://github.com/aicodeking/leanrea) on Arch and NixOS. It is a single
skill that drives plain local CLI tools instead of an MCP server. `pkgs/leanrea`
pins the upstream skill and rewrites its install hints to use the Nix-provided
commands.

Home Manager installs the skill at `~/.config/opencode/skills/leanrea`; the shared
`~/.agents/skills` link exposes it to Codex on the workstations. `modules/home/reverse-engineering.nix`
also installs the Linux-side toolchain: Ghidra, jadx, apktool, radare2, binwalk,
unblob, WABT, tshark, asar, electron-fuses, ilspycmd, cargo-audit, checksec,
Frida, Android tools, binutils, Go, SQLite, ripgrep and archivers. The host-aware
chezmoi `opencode.jsonc` template no longer registers an MCP server for reverse
engineering; Home Manager does not generate `opencode.json`.

Restart agents after a rebuild so they reload their skill lists, then ask for a
reverse-engineering task and let the `leanrea` skill route it.

Apply `homeConfigurations.main` or `homeConfigurations.laptop` on Arch. On NixOS,
use the system rebuild command above. Update this flake to change the installation;
do not install this toolchain with brew, npm, dotnet tool or cargo.

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
cargo test --locked --manifest-path pkgs/host-tools/Cargo.toml
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
cargo build --release --locked --manifest-path pkgs/host-tools/Cargo.toml
sudo python3 arch/maintenance/apply.py
```

The installer uses `pkgs/host-tools/target/release/host-tools` by default. If Cargo
uses a custom target directory, pass its binary path as `HOST_TOOLS_BINARY` to
`sudo`.

The Arch installer copies that binary to `/usr/local/libexec/host-tools` for its
system units. On the Pi, build for its architecture, install the binary as
`/usr/local/bin/host-tools`, and use the units in `aux/rpi`. Replace the old cron
entry when enabling the WoL timer; do not schedule both. Keep that timer disabled
during RTC-only wake comparisons.

Tests for the recurring programs live in `pkgs/host-tools/src/tests.rs` and run
as part of the Nix package build. Python remains only in manual/one-time tools
and their tests; upstream application internals are managed by their packages.

## Laptop RyzenAdj recovery

`ryzenadj-laptop` maintains the laptop's existing limits on AC power in performance
mode. Each application selects `ryzenadj --max-performance` first, then writes the
custom power and temperature limits. This applies on startup, entry into AC
performance mode, readback drift and recovery retries, including after a profile
cycle. Limits are only written on AC power, so the last applied values stay in
place on battery until another platform profile reprograms them.

It samples every five seconds. A sample is suspicious only when the CPU is busy
(at least 60%, excluding I/O wait) while package power is at most 17 W and mean
CPU frequency is at most 1.8 GHz; an idle machine fails the busy check, so low
clocks and low power alone never start recovery. A core at or above 85 °C is
treated as thermal throttling rather than a stuck cap. Two consecutive suspicious
samples reapply RyzenAdj, and three reapply attempts five seconds apart are
followed by one balanced-to-performance cycle when the condition persists and the
ten-minute cooldown allows it.

Recovery requires a core temperature below 85°C and an APU skin reading below
50°C. CPU use excludes I/O wait; memory pressure does not veto recovery when the
CPUs are still busy. These checks are a heuristic for the observed low-power state,
not proof of a firmware fault. Recovery does not disable thermal protection or
raise the configured limits.

Profile cycles have a ten-minute cooldown, saved in the service's state directory
so restarting the service cannot bypass it. Once the recovery commands finish,
the watcher observes for up to three minutes. Five seconds at or above 20 W counts
as recovery even if CPU use falls; high clocks alone count only while CPU use is
at least 60%. An unsuccessful or inconclusive attempt does not disable monitoring
or future recovery. Another sustained low-power episode can trigger an attempt
after the cooldown expires. If the charger or selected profile changes during
recovery, the watcher aborts rather than forcing performance mode.
If returning from balanced to performance fails, it retries that step every five
seconds while the charger and profile checks still permit it.

Read recovery decisions with `journalctl --user -u ryzenadj-laptop` on Arch, or
`journalctl -u ryzenadj-laptop` on NixOS. While a busy CPU is stuck at low power
and clocks, the watcher logs temperatures, recovery state and remaining cooldown
at most once per minute, even when recovery is blocked. Episode start and sustained
recovery are logged separately from command completion, including delayed recovery
after the verification window. These observations do not prove the commands caused
the recovery.

## Main's fan configuration

Main runs one Rust daemon, `host-tools fan-control run`, under
`fancontrol.service`. It reads Ryzen die temperature, AMD GPU edge temperature
and fan feedback directly from hwmon every second. The same process follows
power-profile changes and resume events over D-Bus. Profile changes update the
curve without restarting or reinitializing PWM; resume rediscovers sensor paths
and restores manual control. No Bash controller, GPU helper process or separate
profile watcher is involved.

`hosts/main/fan-control.json` defines the four controlled channels. Three
readings are averaged; a raw CPU or GPU reading at its upper threshold bypasses
averaging and immediately requests maximum cooling. Reloading clears the history.

The CPU group stops below its threshold. `start_hysteresis` adds 3°C before
stopped fans start again; spinning fans stay on until the stop threshold.
The minimum spinning duty is 20%, with a nonblocking 30% start kick for one second.

- Power Saver stops at 75°C, starts at 78°C and reaches its 70% cap at 90°C.
- Balanced stops at 65°C, starts at 68°C and reaches 100% at 85°C.
- Performance stops at 65°C, starts at 68°C and reaches 100% at 80°C.

Power Saver caps every channel at 70%, including the emergency response. In
that profile power-profiles-daemon already disables CPU boost (3.4 GHz base
clock, about 96 W package at full load), which keeps the die near 62°C with the
CPU fans stopped. Lower clock caps were measured and rejected: 2.8 GHz still
draws 88 W and 2.2 GHz 82 W, because the I/O die's share is fixed. At 90°C the
CPU throttles itself, so the lower cap costs speed, not safety.

`case_fans.channels` retains a 30% minimum. These channels follow whichever
requests more airflow: the CPU curve or GPU edge temperature, rising from 30%
at 50°C to the profile's maximum at 80°C. GPU demand is mapped onto the CPU
curve's scale before taking the maximum and averaging. The CPU group follows
only CPU temperature; the GPU keeps control of its own fans.

The explicit role assignment uses channel 2 for the usual ASUS/Nuvoton CPU_FAN
mapping and channels 1, 3 and 4 for case airflow. Individual PWM/RPM control was
checked, but Linux exposes no physical header labels on this board. The reported
wiring puts the NH-D15 on CPU_OPT and the rear case fan on CPU_FAN. Those headers
share control, so that rear fan follows the CPU group's stop/start curve too.
Confirm the cabling when changing fans and update the case channel list if needed.

Each start validates the sensor labels and curve. Invalid or missing sensor data,
invalid fan feedback, failed writes or a fan reporting zero RPM for five seconds
while commanded to spin fail the daemon. Normal exits restore motherboard
automatic control through a Rust recovery guard. Independently, systemd's `ExecStopPost` runs
`host-tools fan-control automatic`, which needs neither the JSON curve nor
Linux temperature providers or D-Bus. It switches each controlled channel to
NCT6798 Smart Fan IV (PWM enable mode 5), using the firmware's existing sensor
selection and hardware curve. The daemon leaves those settings intact. Stopping
the service, including during shutdown, therefore hands control to the motherboard.

The daemon sends systemd a watchdog heartbeat only after a successful control
sample. A stalled loop is killed after ten seconds, then the independent recovery
hook runs. Unrequested exits restart after two seconds, with the start rate limit
disabled so recovery keeps retrying. An explicit systemctl stop leaves the
motherboard in control. This protects against a hung process as well as an exit;
it cannot keep running after an operating-system or hardware failure.

Keep CPU die temperature as the daemon's source: this board's NCT6798 hardware
curve engine cannot select its separate Ryzen TSI input through the driver.
The motherboard fallback uses its firmware-selected sensor instead; its readings
and curve differ from the daemon's CPU/GPU control. The retained firmware curve
on channels 1–4 rises from 60% at 30°C to 100% at 70°C on that sensor.

`hosts/main/fancontrol.service` holds the controller and channel arguments.
It must not depend on `power-profiles-daemon.service`: that daemon is ordered
after `multi-user.target`, while the fan service starts before that target.
Adding the dependency creates a boot ordering cycle. The Rust daemon reads and
subscribes to power profiles over D-Bus, retrying startup if the provider is not ready.

`hosts/main/default.nix` installs the shared unit and loads `nct6775` on NixOS.
Arch's `lm_sensors.service` loads the module from `/etc/conf.d/lm_sensors`;
keep that package for driver loading and diagnostics. Build `host-tools` with
the command above, then install on Arch:

```sh
sudo install -m 0755 pkgs/host-tools/target/release/host-tools /usr/local/libexec/host-tools
sudo install -m 0644 hosts/main/fan-control.json /etc/main-fan-control.json
sudo install -m 0644 hosts/main/fancontrol.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now fancontrol.service
```

After editing the Arch curve, reinstall the JSON and run
`sudo systemctl reload fancontrol.service`. Systemd sends SIGHUP; the daemon
acknowledges completion after validating and applying the new configuration.
Restart the service after replacing its binary or unit. Current profile, raw
temperatures, requested PWM and sampled RPM are available in the root-only
`/run/main-fan-control/status.json`. Home Manager does not own this system service.

## Main's RGB lighting

`hosts/main/main-rgb.service` keeps the Corsair Vengeance RGB Pro DIMMs and
ASUS Aura lighting off. OpenRGB controls only the RAM, using a saved Color
Pulse effect with both colors black and brightness zero. Custom black leaves
these DIMMs white; Direct black does not replace the saved effect. Liquidctl
blanks all Aura RGB/ARGB channels. The private OpenRGB configuration disables
Aura detection to avoid opening the same USB controller twice. Fan speeds are
unchanged.

The oneshot runs at boot and after the suspend, hibernate, hybrid-sleep and
suspend-then-hibernate targets complete. It exits after each run so the next
wake can start it again. No lighting commands run before sleep or during
shutdown, and no RGB daemon runs.
A separate Corsair USB lighting hub has not been detected; lighting on an
undetected controller is not covered.

NixOS installs the unit from `hosts/main/default.nix`. On Arch, keep the
existing `openrgb` and `liquidctl` packages and install the shared unit:

```sh
sudo install -m 0644 hosts/main/main-rgb.service /etc/systemd/system/
sudo install -Dm0644 hosts/main/OpenRGB.json /etc/main-rgb/OpenRGB.json
sudo systemctl daemon-reload
sudo systemctl enable --now main-rgb.service
```

The dedicated configuration retains the disabled keyboard/mouse detectors from
main's working OpenRGB settings. Default detection crashes OpenRGB 1.0 in
hidapi cleanup on main. The configuration is copied into the private
state directory before each start; the interactive OpenRGB settings stay separate.
Arch's existing `rgb-off.service` remains responsible for blanking other
peripherals at boot, before this job. Actual standby darkness still
needs a physical check; motherboard firmware can have separate standby-lighting
settings.
