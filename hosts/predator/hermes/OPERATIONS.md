# Operations

The owner lifted the earlier deployment hold on 2026-10-08 and authorized the
complete reviewed Nix configuration. Reboot and suspend tests remain separate
from deployment. See the acceptance record for current runtime evidence.

## Build and deploy

Run from `~/dev/dotfiles`; these commands also work in Nushell:

```text
nix flake check path:. --no-build --no-update-lock-file
nix build path:.#nixosConfigurations.predator.config.system.build.toplevel --out-link ~/dev/predator-hermes-system
```

Predator has substantial existing uncommitted work. Inspect both checkouts'
`git status` and `git diff`, including `flake.lock`, before syncing. Preserve
newer target pins and pending work; transfer only reviewed source changes.
Do not use whole-tree `rsync --delete`. The 2026-10-08 deployment preserved
Predator's nixpkgs revision and saved its original source tree at
`~/dev/dotfiles-before-hermes-20261008` on Predator and
`~/dev/predator-dotfiles-before-hermes-20261008` on main.

Before any switch, record the running generation and preserve the reviewed
baseline configuration. Confirm that the proposed system applies only approved
changes; a normal rebuild of the dirty target also applies its pending work.
Once authorized, run the target rebuild detached and inspect its unit log and
exit status:

```text
ssh predator 'sudo systemd-run --no-block --unit=hermes-deployment --property=Type=oneshot --property=WorkingDirectory=/home/mirsella/dev/dotfiles --setenv=PATH=/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin nixos-rebuild switch --flake path:/home/mirsella/dev/dotfiles#predator --no-update-lock-file'
ssh predator 'sudo journalctl -u hermes-deployment --no-pager -n 100'
ssh predator 'sudo systemctl show hermes-deployment -p ActiveState -p Result -p ExecMainStatus'
```

A switch is not a reboot. No reboot, suspend, disk formatting, OS migration,
secret rotation or power-policy change is part of this integration.

## Service commands

The operator and Hermes run as `mirsella` and can administer the host through
passwordless sudo. Use the compiled `host-tools` package for lifecycle actions:

```text
ssh predator 'sudo host-tools hermes status'
ssh predator 'sudo host-tools hermes logs'
ssh predator 'sudo host-tools hermes start'
ssh predator 'sudo host-tools hermes stop'
ssh predator 'sudo host-tools hermes restart'
```

Units: `hermes-agent.service`, `hermes-browser-control.service`,
`camofox-browser.service`, `hermes-network-isolation.service`,
`sleev-gateway.service` and `hermes-secret-service.service`.
The first three have five-second failure restart delays and private journals.
The isolation unit gates the browser; the owner's terminal retains normal
network access. Sleev and the unlocked Secret Service start before the gateway.

## Personal account and runtime configuration

Hermes uses the owner's home, dotfiles, SSH keys and account tools. Its state is
still `/var/lib/hermes/.hermes`, preserving existing conversations and schedules.
The installed `hermes` wrapper selects this home and `HERMES_MANAGED=false`.
Runtime `config.yaml` edits are preserved; Nix activation seeds only missing
defaults. Provider endpoints and provisioned secrets in `.env` are generated
from Nix/SOPS on gateway start, so persistent changes to those belong in the
flake/encrypted source.

For account commands outside the gateway, use the same headless keyring context:

```text
ssh predator 'env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus PROTON_PASS_LINUX_KEYRING=dbus pass-cli info'
ssh predator 'env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus gh api user --jq .login'
ssh predator 'env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus protonmail-cli whoami'
ssh predator 'hermes auth list openai-codex'
```

The Pass login is an independent `Predator Hermes` PAT with editor access to the
existing vaults, saved in Personal / Predator Hermes CLI. GitHub reuses the
owner's existing token. OpenAI/Codex uses an independent Hermes OAuth grant.
Do not clone rotating OAuth sessions from another application.

Sleev is packaged by Nix, not installed or updated by the agent's first start.
Use `sleev auth login --device` for a new machine's independent account login.
The system `sleev-gateway` unit starts the patched native payload directly;
vendor `sleev setup` attempts to download an unpatched generic Linux launcher.

The browser remains cold until a real tool or explicit viewer action. To inspect
status, use the authenticated viewer or service log; do not hit private admin
routes from an arbitrary terminal. Browser passwords/MFA are entered in the
viewer, never sent to the bot. For stale CSRF after controller restart, reload.
For startup failure, inspect the private service logs, fix the declared config
and Retry. Do not change headed/headless mode to work around display failure.

The viewer uses the packaged noVNC interface. Connect starts the browser; the
collapsible left control bar provides the phone keyboard, extra keys, clipboard,
fullscreen and scaling settings. Disconnect closes that viewer connection;
normal idle cleanup still controls the browser's lifetime. Visiting the page
does not connect or start the browser automatically.

Browser help is conversational: the agent replies that it is blocked and sends
the viewer URL, then waits. Complete the step and say `done` in the chat; the
agent rechecks with a fresh snapshot. There is no `browser_handoff` tool or
viewer/agent ownership lock. The agent waits for `done` without snapshots,
polling jobs or automatic rechecks. Native `/goal pause` and
`/goal resume` remain available if an autonomous goal needs manual control.

Downloads are temporary. Hermes can read completed files in
`/var/lib/hermes/workspace/downloads` and copy selected files to another workspace
path before idle cleanup. This view is read-only, so the browser remains the
only owner of download deletion. There is no automatic archive; ask explicitly
to keep a file. Profiles, cookies and other browser temporary files are not
shared with the agent account.

## Secrets

Already provisioned: `secrets/hermes.yaml` contains the four separate runtime
environments, Go/Zen keys and a separate Secret Service password. No existing
key was rotated. The viewer login is in `~/.config/hermes/viewer.json` on main
and Personal / Predator browser in Proton Pass.

If provisioning a fresh checkout, build `host-tools` and `ssh-to-age`, then run
the Rust provisioner as the owner, not root:

```text
nix build path:.#host-tools --out-link ~/dev/hermes-host-tools
nix build --impure --expr '(builtins.getFlake "path:/home/mirsella/dev/dotfiles").inputs.nixpkgs.legacyPackages.x86_64-linux.ssh-to-age' --out-link ~/dev/hermes-ssh-to-age
~/dev/hermes-host-tools/bin/host-tools hermes-provision --root ~/dev/dotfiles --ssh-to-age ~/dev/hermes-ssh-to-age/bin/ssh-to-age --reuse-mirsellabot
```

With existing secrets, this preserves saved credentials, fills a missing Zen
key/Secret Service password and updates the verified bot/owner mapping. Never
regenerate secrets merely because they were read. Use SOPS for intentional changes.
Secret changes restart their consumer and its dependents. Model/Telegram changes
restart only the gateway; browser changes also restart the controller and gateway.
Caddy changes restart only Caddy.
Fresh provisioning stages the encrypted secrets and private viewer login before
publication. Failed attempts clean staged files and roll back their new login;
existing credentials are never overwritten by fresh provisioning. Bot reuse
updates the existing encrypted source atomically.

## Native reminders and automation

Request reminders in the authorized Telegram DM. Native schedules survive
gateway restart; upstream missed-job catch-up policy applies. Remove temporary test
jobs after verification. Telegram also drops queued updates on a cold gateway
start. Network reconnects retain updates; messages sent while Predator is
suspended can arrive on wake.
Do not add permanent heartbeats or monitors unless asked.

The deployment uses Hermes's standard tools, approval flow and scheduler.
No custom monitor is installed and no cron execution path is patched.
For new host automation, write Rust and declare the package/service/timer in
Nix. Hermes can edit and deploy the flake using sudo. Compile deployed programs
during the Nix build. Nightly
`-Zscript` is for one-off diagnostics, not first-start compilation of services.

## Backup, restore and rollback

```text
ssh predator 'sudo host-tools hermes backup'
ssh predator 'sudo host-tools hermes restore --archive /var/lib/hermes-backups/hermes-TIMESTAMP.tar.gz'
```

Backup stops active integration units and the owner's native
`hermes-worker-*.scope` workers for a consistent database/browser
checkpoint, archives gateway/browser/controller state including
ownership/xattrs, then restarts only previously active units. Backups are
root-only on the SSD and contain private credentials/profile data. They are
manual, not an additional nightly HDD wakeup or off-machine backup. Copy an
encrypted backup off-machine deliberately when needed.
The private archive destination is prepared before stopping services. Previously
active services are restarted even after a partial stop or archive failure;
recovery errors are reported alongside the original failure. Publication never
overwrites an existing backup. Failed partial archives remain for diagnosis.
The temporary download directory is excluded; explicitly copied workspace
files are included. The owner's user manager and other services stay running.
This archive covers the three application state roots, not personal home,
Secret Service, Sleev or account-tool state.

Backups are validated for restore compatibility before publication. Restore
validates archive locations, member paths/types and link targets, then extracts
into a root-only staging directory before stopping services. It replaces whole
state directories, so files absent from the archive do not survive. It retains
the previous trees in a root-only `/var/lib/.hermes-before-restore-*` directory
and rolls back a partial replacement on failure. Recovery errors include the
retained paths. Lifecycle/backup/restore commands share an exclusive lock.
Services remain stopped after restore. Rebuild the reviewed configuration to
reinstall Nix-managed instructions and seed missing defaults, then explicitly
start. Runtime model choices remain editable. Do not run `tar` over live
SQLite databases. Exercise recovery on disposable state before relying on it.

The initial validated baseline is
`/var/lib/hermes-backups/hermes-20261008T131006.963800306Z.tar.gz` on Predator.
An age-encrypted off-machine copy is in
`~/dev/hermes-backups-predator-20261008/` on main, using the three existing
SOPS recipients. Full decryption/authentication verification passed without
writing a plaintext archive on main. This is a manual baseline, not a recurring
backup job.

A second consistent archive was taken immediately before owner migration:
`/var/lib/hermes-backups/hermes-20261008T144410.050812770Z.tar.gz`.
The owner-era baseline is
`/var/lib/hermes-backups/hermes-20261008T155703.519033928Z.tar.gz`;
its backup left the owner's user manager running with the same PID.

For a bad system switch, review the previous generation and use the standard
`sudo nixos-rebuild switch --rollback`. Restore state separately if an upstream
database migration changed its format. For a package update, change the pinned
Hermes input or Camofox/engine revision/hash, regenerate the runtime-only npm
lock/hash, inspect the compatibility substitutions and rerun focused acceptance.
No gateway self-update or runtime package installation is supported.

## Power and measurement

Predator's existing nightly suspend policy is preserved. The assistant/viewer
is unavailable while the host sleeps. Do not assume a handoff or a running job
blocks suspend: that interaction is a pending acceptance test, after the
owner's independent overnight test. Never claim a live challenge survives
suspend/restart without checking it.

After deployment, collect actual process trees, per-process PSS and cgroup CPU
for at least several minutes each: control-only, quiet browser without viewer,
connected inactive viewer, interactive use, and return to idle. Use private
`systemctl show ... -p ControlGroup -p MemoryCurrent -p CPUUsageNSec`,
`systemd-cgtop`, `ps`, and root-readable `/proc/PID/smaps_rollup`. Sum each PID
once; cgroup totals and per-process PSS are different measurements and must not
be added together. Record cold-start latency, ten-minute cleanup and watcher
overhead. Check logs for retries/requests and verify idle work causes no model
requests. Record measured limits/headroom before adding resource caps.

The acceptance record contains initial browser-off PSS/CPU samples. The remaining
phases and model-idle verification are pending. Watts require a wall meter or
battery-discharge measurement; do not infer them from CPU/RAM alone.
