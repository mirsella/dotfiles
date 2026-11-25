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
- Main uses one Rust CPU/GPU fan daemon with D-Bus profile/resume handling.
  Save its ramps in `hosts/main/fan-control.json`; its `fancontrol.service` unit
  is shared by NixOS and Arch. Keep CPU die temperature as the CPU source.
  Preserve independent motherboard automatic ExecStopPost recovery and the
  systemd watchdog. Leave firmware sensor selections and Smart Fan IV curves
  intact so automatic control can take over on every exit or watchdog failure.
  Never order the fan unit after power-profiles-daemon: it is ordered after
  multi-user.target, so ordering after it is a boot ordering cycle that makes
  systemd delete the fan start job. Keep its start rate limit disabled
  (`StartLimitIntervalSec=0`): the fan failsafe has to keep retrying
  instead of staying dead.
- Chezmoi owns editable app configuration. Home Manager owns packages, generated
  Git/SSH settings, user services and SOPS wiring. Keep each path under one
  owner; `.chezmoiignore` lists the HM-owned paths. Predator uses this same
  checkout as its chezmoi source, but activation never applies it: run
  `chezmoi apply` there after changing managed files and watch `chezmoi status`
  for drift, since a stale copy there is otherwise invisible.
- Home Manager's plasma-manager owns the Plasma settings in
  `modules/home/plasma.nix`: panel, desktop widgets, wallpaper, global
  shortcuts and the kwin/input/krunner/power/lock-screen rc files. Main and
  laptop share it; their `home.nix` sets only screen size, sensors and
  hardware-specific keys. It writes declared keys at activation, so Plasma's
  runtime state no longer shows as drift. Port GUI changes to the module;
  nothing captures them. The layout scripts rebuild the panel and widgets at the
  next login only when they change; run
  `~/.local/share/plasma-manager/run_all.sh` to apply them now (it restarts
  plasmashell). kwin rewrites `kglobalshortcutsrc` from memory whenever an app
  registers a shortcut, so a shortcut change only sticks after the next login,
  or when applied live through kglobalaccel's `setForeignShortcutKeys`.
- OpenCode extensions and their tests live in `pkgs/opencode-extensions`; Home
  Manager installs the compiled plugins. Its editable JSON settings remain in
  chezmoi. The shared `pkgs/idle-watchdog` Cargo project builds both the OpenCode
  and T3 watchdog commands. The OpenCode side discovers only `opencode serve`
  processes and requires their v2 API.
- T3's portable settings (`~/.config/t3code/portable-settings.json`) live in
  chezmoi; `t3-settings` captures them on `chezmoi re-add` (skipping when T3 is
  stopped) and applies repository changes on `chezmoi apply` without capturing
  live-only changes. Conflicts need a force flag, then `re-add` to save a
  live-wins capture. `t3 update` repoints T3's tracked CLI launchers and the
  service; desktop-app updates do not update `~/.local/bin/t3`. `t3-settings`
  runs the server's runtime CLI, so sync does not depend on the launcher and
  there is no settings watcher. Apply refuses a fallback CLI whose version
  doesn't match the server because it would migrate the newer database; an
  unreadable version only warns.
- Agent workspace cleanup has one owner: `sweep` in `pkgs/host-tools`, run
  hourly as `sweep auto` by the `sweep` user timer (`modules/home/agent-tools.nix`).
  After 3 idle days it removes task scratch, T3 scratch folders and worktrees and
  OpenCode worktrees; after 7, linked worktrees under `~/dev`. Plain folders
  among the worktrees go like scratch. Before removal it saves what Git would
  lose: uncommitted files (up to 100 MiB, ignored files excluded) and a HEAD on
  no ref become a local, never pushed `sweep/wip/<date>-<name>` branch committed
  through a scratch index, and a nested clone with unpushed commits, a stash or
  saved WIP becomes a bundle in `$XDG_DATA_HOME/sweep/bundles`. Edits already
  on the remote default branch need no saving. `sweep wip` lists saved work and
  how to restore it; it is dropped after 30 days (`saved.json`). Git
  repositories under package caches (`.build`, `node_modules`, `target`,
  `.cargo`, `.gradle`, `.pnpm-store`) are not work. Files another user owns are
  removed through passwordless `sudo -n`. A removed worktree's branch is
  deleted once the default branch contains it, or forgotten after 60 days
  (`branches.json`). `git worktree lock` keeps one indefinitely. An idle
  workspace that still cannot be saved (too many uncommitted bytes, a broken or
  bare repository, a nested worktree outside it) raises one desktop
  notification until it changes; `$XDG_STATE_HOME/sweep/reported.json` remembers
  what was reported. That notification waits in a transient `sweep-attention-*`
  unit. In Plasma, the popup does not time out. Minimize keeps it in notification
  history; Close removes it. Its actions work while the unit is running. Copy
  puts the list, with links to the T3 threads bound to each workspace, on the
  clipboard through Klipper. Ask T3 Code starts a thread in `~/dev/dotfiles` on
  that host's T3 server that explains each one and links the threads behind it,
  then opens it through
  `t3code://threads/<environment>/<thread>`. It keeps anything it cannot verify
  and fails, with a notification, when T3's data changes shape. With no
  arguments `sweep` lists every workspace with its branch and path from `~`,
  when it goes or why it stays, and totals; `sweep status --json` emits typed
  rows that Nushell's `sweep` (`dotfiles/private_dot_config/nushell/sweep.nu`)
  turns into a sortable table; `sweep clean 1h` (or `30m`, `2d`,
  `today`) removes everything idle that long after a confirmation. `sweep keep
  PATH [--for 2d]` protects a managed workspace or a file or directory inside
  it. The hold leaves the whole workspace in place, without copying anything.
  `sweep keep` lists holds (`keep.json` next to `reported.json`);
  `sweep unkeep PATH` releases a hold by the original path. Remove a workspace
  by hand with `git worktree remove --force` or `rm -rf` for plain folders.
  Runs share a lock, so a manual clean never races the timer. Idleness comes
  from cheap signs, checked first: a workspace's own and nested directories'
  times, Git's administrative files (HEAD, index, reflog), the T3 thread's
  activity and OpenCode sessions; in-place file edits are not looked at. Only a
  worktree idle and clean by those signs is walked, and the walk lists
  directories, not files, while T3's state loads. Sizes in `sweep` are cached in
  `sizes.json` until those signs change. T3's portable
  `server.storageCleanup` sets only its log and browser-artifact retention; keep
  T3's worktree rules unset so its own worktree cleanup stays off.
- OpenCode's skill files are the canonical prompts, owned by chezmoi. Edit them
  in `~/.config/opencode/skills` and use `chezmoi re-add` to save edits. Home Manager
  links `~/.agents/skills` to that directory for Codex and Hermes, and links each
  skill into `~/.claude/skills` plus the global instructions to
  `~/.claude/CLAUDE.md` for Claude Code, which never reads `~/.agents`. Claude's
  skill links are enumerated when the flake is evaluated, so a new skill reaches
  Claude only after it is added to Git and Home Manager rebuilds. The shared links
  are ignored by chezmoi; do not track copies or turn skills into templates.
- Retire one-time migrations after every affected host has completed them. Keep
  deployment evidence in Git history rather than permanent acceptance reports.
- Chezmoi auto-commits and auto-pushes source edits. Its Git operation can include
  unrelated changes in this shared repository, including staged Nix changes.
  Review or finish those changes before `chezmoi add`, `edit` or `re-add`.

- Enable OpenCode providers explicitly in the `policies` allowlist. Keep the
  workstation-only `llama.cpp` provider on direct loopback to Polaris, not Sleev.
  OpenCode handles web search separately.
- OpenCode, Codex, Claude Code and T3 use writable standalone installs
  bootstrapped by `programs.agent-tools`; activation only seeds missing installs
  and never replaces or downgrades them, and T3 can update the native agent
  binaries. OpenCode and Codex fetch their latest releases, and T3's CLI and
  AppImage fetch the latest nightly, only on first install. Bootstrap leaves
  their existing launchers and partial installs alone. On Arch,
  pacman/AUR own other application binaries except Hex, which Home Manager installs
  from this flake on both Arch and NixOS. The HM profiles install config
  and services, and kache from the flake so the daemon and cargo wrapper always
  match. On NixOS, rebuild the system target, never activate a standalone
  HM profile for the same user. The standalone homes remain available for Arch.
- Predator's daily upgrade checks for an idle host from 04:00 to 06:00
  and requires 30 GiB free on root. It refreshes only nixpkgs from the
  deployed source in the Nix store without writing a lock file or rebooting.
  Before a manual deployment, stop `nixos-upgrade.timer` and wait for any
  queued or running `nixos-upgrade.service` to finish; restore the timer
  after the deployment if it was active before.
  Before deploying to Predator, inspect its `~/dev/dotfiles` status and diff,
  including any `flake.lock` edits left by the old upgrade job. Sync only
  reviewed changes; do not use whole-tree `rsync --delete` over concurrent work.
  Root cannot evaluate this checkout (a Git flake owned by another user), so
  build as `mirsella` and activate as root: `nix build` the
  `nixosConfigurations.predator.config.system.build.toplevel` output with
  `--print-out-paths --no-link`, then `sudo nix-env -p
/nix/var/nix/profiles/system --set` that path and run its
  `bin/switch-to-configuration switch`. A `systemd-run` user unit has no `nix`
  on PATH, so run the build over ssh. Poll a long build's exit status instead
  of restarting it. Its Hermes workspace `AGENTS.md` is re-seeded from
  `hosts/predator/hermes/agent-AGENTS.md` on every activation, so keep
  instruction edits in the repository.
- Predator's initrd OS separator and signed SHA-256 PCR 11 policies protect all
  nine LUKS volumes on six physical disks. Root unlocks at `enter-initrd`; data unlocks at
  `enter-initrd:leave-initrd`, before the host sysinit phase. Keep the Nix-managed
  Lanzaboote signature companion hook and phase ordering intact. After reboot,
  `sudo predator-boot audit` proves token use and late-phase rejection. Preserve
  `/var/lib/predator-boot` and `/var/lib/sbctl`; see README for recovery.
- Use Git-backed flake references (`.`, `.#package` or the checkout's absolute
  path without `path:`) so Nix excludes `.agents-local/`. Add new source files to
  the Git index before evaluating; they do not need to be committed.
- Run `nix eval .#ci.drvPath --raw --no-update-lock-file` before
  `nix flake check . --no-build --no-update-lock-file`, especially on a cold
  store or after GC: the check is read-only, but Hermes reads metadata from
  generated, filtered source paths. The eval materializes those sources without
  building the CI packages. The flake check evaluates all profiles
  and runs the host-isolation and Predator boot/storage invariants through `checks`. GitHub Actions builds
  `.#ci` on pushes to `main`: every declared check, host-tools with its Rust tests,
  and camofox-browser. Format `pkgs/host-tools` with the flake's pinned
  rustfmt (`nix fmt`); a newer local rustfmt can reformat code the gate accepts.
  Run focused Cargo tests for monitoring, backups and suspend changes. In
  host-tools tests, create scripts and other executables with
  `test_support::executable`, never `fs::write` plus chmod: a parallel test's
  fork can hold the write handle and fail the run with `ETXTBSY`. The
  `data-unlock` and `arch-maintenance` checks run the tests under `tests/` during
  `nix flake check` without `--no-build`.
- `disko.nix` formats **all five data disks** (the SSD and four HDDs) only for a fresh, intentional
  installation. To reinstall on existing disks, mount them without running disko,
  preserve the LUKS headers, ZFS pools, Secure Boot signing bundle (`/var/lib/sbctl`),
  TPM policy authorities (`/var/lib/predator-boot`), and SOPS host identity (`/etc/ssh/ssh_host_ed25519_key`),
  then install `#predator`.
  `tank` uses four roughly 750 GB encrypted HDD sections in RAIDZ1; `backups`
  uses the three roughly 250 GB tails in another RAIDZ1. Both survive any one
  physical HDD failure. Replacing a 1 TB HDD requires replacing its encrypted
  members in both pools. The backup pool protects SSD originals, not an
  independent copy of the HDD archive. Database dumps and ZFS snapshots under
  `backups/data` are on-machine only. SMART and ZED send storage warnings via Resend.
  Do not duplicate `tank/archive` into the backup pool. Keep its local snapshots.
  Predator's interactive `rm` uses gtrash; use `gtrash put` explicitly for ad-hoc
  deletion in noninteractive agent shells. Native deletion in scripts remains
  permanent. Daily root/user timers prune Trash after 30 days; ZFS snapshots may
  retain the blocks longer.
- A nightly restic backup copies `fast/data` and `fast/ncdata` from throwaway
  `.zfs/snapshot/restic-backup` snapshots to `/srv/backup/restic` at 23:50.
  Recovery bundles run Sundays at 23:10 under
  `/srv/backup/recovery/latest`: current LUKS headers and a private archive of
  TPM policy authorities, the SOPS host identity and Secure Boot signing keys. Keep this directory root-only.
  Database dumps use `/srv/backup/recovery/db/latest` on the same private dataset;
  all jobs publish atomically, retain 7 complete bundles, and ping healthchecks.io on success.
- Data disks unlock through optional runtime crypttab units, not initrd. All data
  datasets use systemd-managed legacy mounts with `nofail`. Keep missing data disks
  nonfatal to boot, allow degraded RAIDZ1 imports, and gate writers on their mounts.

Manual files outside the repo:

- Laptop uses Plasma lid defaults. Predator stays awake when idle;
  `hosts/predator/lid-screen.nix` blanks only its local screen on lid close.
- Arch boxes: root-owned Nix daemon GC/cache settings, Timeshift retention,
  storage timers, the fail2ban SSH jail and the `/usr/local/bin/pass-cli`
  file-key wrapper are installed from `arch/maintenance/apply.py`. System
  Caddy (`/etc/caddy/Caddyfile`, `caddy.service`), udev rules and pacman hooks
  remain manual. The Nix daemon ignores cache settings from an untrusted Home
  Manager user config.
- Predator: Freebox LAN IP is `192.168.1.1`; SMART runs daily short and weekly
  long tests on all six physical drives. Toshiba's earlier USB setup had repeated
  host-aborted long tests after about 10 minutes; check its adapter, cable and power
  if this recurs.
- rpi (Debian) is outside the flake; its fail2ban SSH jail is tuned by hand in
  `/etc/fail2ban/jail.d/sshd.local` with the same values as the workstations.

## Nextcloud access

On main and laptop, use the configured `nextcloud:` rclone remote directly.
Home Manager installs its SOPS-managed credentials at
`~/.config/rclone/rclone.conf`. Do not print the config or copy credentials into
commands, URLs or tracked files. On other hosts, check `rclone listremotes`
before assuming the remote exists.

```sh
rclone lsf nextcloud: --dirs-only                 # discover folders and external drives
rclone lsf 'nextcloud:Documents/'                 # list one folder
rclone lsjson 'nextcloud:Documents/'              # structured listing for agents
rclone cat 'nextcloud:Documents/notes.txt'         # read a small text file
rclone copyto 'nextcloud:Documents/report.pdf' ./report.pdf
rclone copyto ./report.pdf 'nextcloud:Documents/report.pdf' --dry-run
```

For edits, download only the required files into a task directory under `~/dev`,
edit locally, then upload to the exact remote path with `copyto`. Uploads can
overwrite existing files; check for remote changes before replacing them and
remove `--dry-run` only for an intended write. Use `--max-depth` or a narrow path
instead of recursively listing the whole account. For unattended operations,
prefix the command with `timeout 2m` and bound retries with
`--retries 1 --low-level-retries 1 --contimeout 10s --timeout 30s`.

Do not use `rclone sync`, `move`, `delete` or `purge` without authorization for
the affected files. Do not write directly to Nextcloud's server-side data
directory, which bypasses its file index. A 502 response is a server/proxy
failure; report it rather than changing credentials or creating a mount.

Dolphin's Network section contains a Nextcloud WebDAV shortcut, installed by
`modules/home/workstation.nix`. It uses KWallet for credentials and can be added
to Places through Dolphin. Keep Nextcloud unmounted: an unavailable rclone/FUSE
mount can block local trash scans. `~/Documents/Nextcloud` is not a cloud path.
The `dolphin-nextcloud` sync-client plugin is not needed for WebDAV browsing.

## LAN machine map (`~/.ssh/config` aliases)

| alias    | address                               | purpose                                                         |
| -------- | ------------------------------------- | --------------------------------------------------------------- |
| rpi      | 192.168.1.166                         | Raspberry Pi 4, Debian — hosts my PC power-control project      |
| laptop   | 192.168.1.61                          | Framework 13, Arch Linux — work laptop                          |
| main     | `mirsella.mooo.com:2222` (LAN `.131`) | main tower, Arch Linux — powerful desktop                       |
| predator | 192.168.1.19                          | NixOS — next home server (drive, Nextcloud, …), this repo's box |

## SSH public keys (user identity)

`~/.ssh/id_ed25519.pub` of each machine, to copy into an `authorized_keys`
file, a service account or a Git host. Main, laptop and predator are also
registered on the GitHub account; rpi is not.

| device   | public key                                                                         | SHA256 fingerprint                                   |
| -------- | ---------------------------------------------------------------------------------- | ---------------------------------------------------- |
| main     | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe` | `SHA256:Mcc/nTQqdEAlOiPFRj6yHYf4exhB/iM5CwoxcckKpPg` |
| laptop   | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF` | `SHA256:Y8Xmd8d+/SfbsiyozITkC13MVP5MKrwlllz1cw58z4E` |
| predator | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHALDtwhSj8AF8NdRRjcWcDkD/EeDnRcWYjRSbXSznTo` | `SHA256:/6MNKC1SnVco8IlGO6Xe3FDh0BWlE6U7y+fPrAUK82c` |
| rpi      | `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJRadFVhpJRbD98sHFgcVAaMDL48x27On4XAe//Yx6Vm` | `SHA256:4vEJhi7Q5OOw8M4jArYQLG/0x4EID933Ohxsc0Jf6vQ` |
