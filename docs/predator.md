# Predator operations

The local source checkout is `~/dev/dotfiles`; Predator currently deploys from
`~/dev/nixos`. Copy only intended changes when another session is working on
the server. Inspect both worktrees before replacing configuration files.

## Scheduled work

Times use Europe/Paris unless noted. List actual deadlines with
`systemctl list-timers --all`.

| Job | Schedule | Policy |
| --- | --- | --- |
| Database/configuration backup | Daily 23:00 | 14 complete sets on `tank/backup`; catches up after downtime |
| Sanoid snapshots | Daily 23:30 | 7 daily and 4 weekly snapshots of `fast/ncdata`, `fast/data`, `tank/archive`, `tank/backup` |
| Conditional suspend | Every minute from 00:00 through 06:59 | Skip if someone is logged in, a web connection is open, a web request arrived in the last 30 minutes, or maintenance is active; wake by RTC at 07:00 |
| NixOS updates | Sunday 10:00, up to 30 minutes later | Update stable `nixpkgs` as the checkout owner, build and switch; no automatic reboot |
| Root SSD trim | Sunday 11:00 | Native fstrim service |
| ZFS trim | Sunday 11:30, up to 15 minutes later | Native ZFS trim service |
| ZFS scrub | First of each month, 10:00, up to 30 minutes later | Both pools |
| Nextcloud heavy background work | Starts 06:00 UTC | Nextcloud maintenance window |

Sanoid snapshots unchanged datasets as well. ZFS shares their existing data
blocks. Snapshot retention no longer depends on filesystem events or dirty flags.
Snapshots live on the same pools as their source data and do not replace an
off-machine backup. Database dumps and restore details are in [immich.md](immich.md).

## Local access

The screen blanks after 60 seconds of console inactivity; a keypress restores
the TTY. The Intel display driver remains available. NVIDIA runtime power
management is left to its driver.
Closing the lid does not suspend predator. From midnight to 07:00,
`night-suspend` runs once a minute and checks SSH and local logins, open SSH/web
connections, Caddy access logs for the preceding 30 minutes, running
backups/maintenance and Nix builds.
It does not inspect disk activity. If busy, it tries again the next minute.
When idle, it sets the 07:00 local RTC wake alarm before requesting suspend.
The Raspberry Pi sends a wake-on-LAN packet at 07:00 as a fallback if the RTC
wake fails.
Nextcloud's default web session keepalive sends a browser heartbeat about every
five minutes, so an open tab normally keeps the server awake through the
30-minute request window. A suspended or disconnected browser cannot signal
that its tab is open.

Ethernet magic-packet wake is a NetworkManager connection default. A saved
connection's explicit wake-on-LAN setting takes precedence. Changing this
default does not require disconnecting an active interface. The Raspberry Pi's
scheduled wake and manual wake-on-LAN are available.

OpenCode's Home Manager unit uses `X-SwitchMethod=keep-old`. Rebuilds install
new unit definitions while retaining a running instance. Restart it explicitly
when the active sessions can end:

```sh
systemctl --user restart opencode.service
```

## Application configuration

Nextcloud uses PHP workers capped at 512 MiB each, at most four workers, and a
separate 1 GiB CLI limit. Uploads can still be 16 GiB. Startup requires its ZFS
data mount and decrypted secrets. Pinned extra apps and the built-in external
storage, TOTP and suspicious-login apps are enabled during setup.

Sharing defaults, external-storage usage accounting, account recovery and client
preferences are documented in [Nextcloud preferences](nextcloud.md).

Caddy redirects DAV and discovery URLs into `/nextcloud`, preserves Nextcloud's
frame policy, and serves HTTP/1.1 and HTTP/2. HTTP/3 is disabled because the
firewall only permits TCP/443.

## Monitoring (Beszel)

The hub serves at `https://mirsella.mooo.com/beszel` through Caddy; hub and
agent bind localhost only. The agent reports root, `Fast-SSD`, `Archive-HDD`,
and `Nextcloud` usage (the ZFS datasets via `zfs list`). The `nextcloud`
group grants access to the 0750 datadir. SMART monitoring shows the four
disks on `/beszel/smart`. Device cgroup rules permit the disks and `/dev/zfs`;
the USB bridges require `:sat` passthrough. Beszel reads SMART attributes
hourly; it does not schedule self-tests.

`services.smartd` in `modules/nixos/storage.nix` schedules short self-tests
at 12:00 on all four disks and checks SMART every 30 minutes by default.
Failures mail `mirsella@protonmail.com` once via Resend.

`beszel-setup.service` configures the state PocketBase keeps out of NixOS
options on every switch: Resend SMTP (key shared with Nextcloud, sender
`Beszel <noreply@voxride.com>`), the hub admin account, and the `predator`
system entry. Secrets live in `secrets/beszel.yaml` (`heartbeat_env`,
`superuser_password`). First-ever bootstrap (empty state dir) still needs
`beszel-hub superuser upsert` with the hub stopped; the login itself is in
Proton Pass under `Beszel (predator)`.

Heartbeat to Healthchecks.io is live: `HEARTBEAT_URL` in `heartbeat_env`
(empty value disables it). The Healthchecks side is a cron check
`* 7-23 * * *`, `Europe/Paris`, 5 minute grace, excluding the overnight
suspend window (even when activity keeps the machine awake).
Notifications go to both email (`mirsella@protonmail.com`) and Telegram
(paired via `@HealthchecksBot` to the `predator` project, same as voxride).

## Checks

```sh
nix flake check --no-build --no-update-lock-file
nix build --no-update-lock-file --max-jobs 1 --cores 4 \
  .#nixosConfigurations.predator.config.system.build.toplevel
python3 tests/night-suspend.py
sudo systemctl --failed
sudo nextcloud-occ setupchecks
curl --fail https://photos.mirsella.mooo.com/api/server/ping
```

Flake checking without a build only evaluates configuration. Changes to boot
parameters and EFI permissions also need a later boot or mount verification.
