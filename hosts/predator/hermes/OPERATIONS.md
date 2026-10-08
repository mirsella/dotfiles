# Operations

Predator has not been changed. The owner requested no activation or reboot
before the overnight auto-sleep test. The commands below are for a later,
explicitly authorized deployment and routine operation.

## Build and deploy

Run from `~/dev/dotfiles`; these commands also work in Nushell:

```text
nix flake check path:. --no-build --no-update-lock-file
nix build path:.#nixosConfigurations.predator.config.system.build.toplevel --out-link ~/dev/predator-hermes-system
```

The local and Predator checkouts have different revisions/lockfiles, and
Predator has substantial existing uncommitted work. Inspect both `git status`
and `git diff`, including `flake.lock`, before syncing. Preserve the target's
newer nixpkgs pin and pending work. Transfer only reviewed integration files,
Rust sources/lockfile, encrypted secrets, host import and Hermes flake-input
changes. Do not use whole-tree `rsync --delete` or copy the local lockfile over
the target's lockfile.

Before any switch, record the running generation and preserve the reviewed
baseline configuration. Confirm that the proposed system applies only approved
changes; a normal rebuild of the dirty target also applies its pending work.
Once authorized, run the target rebuild detached and inspect its unit log and
exit status:

```text
ssh predator 'sudo systemd-run --unit=hermes-deployment --property=Type=oneshot --property=WorkingDirectory=/home/mirsella/dev/dotfiles nixos-rebuild switch --flake path:/home/mirsella/dev/dotfiles#predator'
ssh predator 'sudo journalctl -u hermes-deployment --no-pager -n 100'
ssh predator 'sudo systemctl show hermes-deployment -p ActiveState -p Result -p ExecMainStatus'
```

A switch is not a reboot. No reboot, suspend, disk formatting, OS migration,
secret rotation or power-policy change is part of this integration.

## Service commands

The operator uses the compiled `host-tools` package. The agent cannot administer
the host. All lifecycle actions use system units, not ad hoc Firefox launches:

```text
ssh predator 'sudo host-tools hermes status'
ssh predator 'sudo host-tools hermes logs'
ssh predator 'sudo host-tools hermes start'
ssh predator 'sudo host-tools hermes stop'
ssh predator 'sudo host-tools hermes restart'
```

Units: `hermes-agent.service`, `hermes-browser-control.service`,
`camofox-browser.service`, and `hermes-network-isolation.service`.
The first three have five-second failure restart delays and private journals.
The isolation unit fails closed before either gateway or browser starts.

The browser remains cold until a real tool or explicit viewer action. To inspect
status, use the authenticated viewer or service log; do not hit private admin
routes from an arbitrary terminal. Browser passwords/MFA are entered in the
viewer, never sent to the bot. For stale CSRF after controller restart, reload.
For startup failure, inspect the private service logs, fix the declared config
and Retry. Do not change headed/headless mode to work around display failure.

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
environments. No key was rotated while reusing `@mirsellabot`. The private viewer
login remains in `~/.config/hermes/viewer.json` on the workstation.

If provisioning a fresh checkout, build `host-tools` and `ssh-to-age`, then run
the Rust provisioner as the owner, not root:

```text
nix build path:.#host-tools --out-link ~/dev/hermes-host-tools
nix build --impure --expr '(builtins.getFlake "path:/home/mirsella/dev/dotfiles").inputs.nixpkgs.legacyPackages.x86_64-linux.ssh-to-age' --out-link ~/dev/hermes-ssh-to-age
~/dev/hermes-host-tools/bin/host-tools hermes-provision --root ~/dev/dotfiles --ssh-to-age ~/dev/hermes-ssh-to-age/bin/ssh-to-age --reuse-mirsellabot
```

With existing secrets, this preserves model/browser/viewer credentials and
only updates the verified bot/owner mapping. Never regenerate secrets merely
because they were read. Use SOPS for intentional changes. Caddy secret changes
restart Caddy; other service secret changes restart the private integration.

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
Nix for owner review. Compile deployed programs during the Nix build. Nightly
`-Zscript` is for one-off diagnostics, not first-start compilation of services.

## Backup, restore and rollback

```text
ssh predator 'sudo host-tools hermes backup'
ssh predator 'sudo host-tools hermes restore --archive /var/lib/hermes-backups/hermes-TIMESTAMP.tar.gz'
```

Backup stops active integration units and the dedicated user's restart-safe
cron worker manager for a consistent database/browser
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
files are included.

Restore validates archive locations/member paths, stops services and preserves
ownership. It leaves them stopped. Rebuild the reviewed configuration to
reinstall immutable managed config, then explicitly start. Do not run `tar`
over live SQLite databases. Test restore on disposable state before relying on
it; production restore has not been exercised yet.

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
be added together. Record cold-start latency, five-minute cleanup and watcher
overhead. Check logs for retries/requests and verify idle work causes no model
requests. Record measured limits/headroom before adding resource caps.

There are no measured Predator idle or power figures yet. Watts require a wall
meter or battery-discharge measurement; do not infer them from CPU/RAM alone.
