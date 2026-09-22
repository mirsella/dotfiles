# Predator operations

The local source checkout is `~/dev/dotfiles`; Predator currently deploys from
`~/dev/nixos`. Copy only intended changes when another session is working on
the server. Inspect both worktrees before replacing configuration files.

## Scheduled work

Times use Europe/Paris unless noted. List actual deadlines with
`systemctl list-timers --all`.

| Job | Schedule | Policy |
| --- | --- | --- |
| Database/configuration backup | Daily 23:00 | 14 complete sets on `tank/backup`; catches up after downtime; inhibits sleep |
| Sanoid snapshots | Daily 23:30 | 7 daily and 4 weekly snapshots of `fast/ncdata`, `fast/data`, `tank/archive`, `tank/backup` |
| NixOS updates | Sunday 10:00, up to 30 minutes later | Update stable `nixpkgs` as the checkout owner, build and switch; no automatic reboot |
| Root SSD trim | Sunday 11:00 | Native fstrim service |
| ZFS trim | Sunday 11:30, up to 15 minutes later | Native ZFS trim service |
| ZFS scrub | First of each month, 10:00, up to 30 minutes later | Both pools; blocks nightly suspend while running |
| Nextcloud heavy background work | Starts 06:00 UTC | Nextcloud maintenance window, after the local morning wake time |

Sanoid snapshots unchanged datasets as well. ZFS shares their existing data
blocks. Snapshot retention no longer depends on filesystem events or dirty flags.
Snapshots live on the same pools as their source data and do not replace an
off-machine backup. Database dumps and restore details are in [immich.md](immich.md).

## Sleep and local access

The screen blanks after 60 seconds of console inactivity; a keypress restores
the TTY. The Intel display driver remains available. NVIDIA runtime power
management is left to its driver.

The idle check samples the two HDDs and the media SSD every five minutes during
the day and every minute between midnight and 07:00. Media I/O keeps the machine
awake during background photo processing, including I/O still in flight. One
shared idle clock uses kernel uptime, so wall-clock corrections cannot shorten
the quiet period. State lives under `/run/night-suspend`; each reboot starts a
fresh 45-minute quiet period.
It blocks sleep for user login sessions, established SSH/application TCP
connections, Nix build workers, and the configured maintenance services.
The final suspend request honors system sleep inhibitors and requires a
successfully programmed 07:00 RTC alarm.

Both USB HDDs use 12-hour SMART polling because their bridges cannot reliably
report standby. SMART checks can still spin up a disk; the idle monitor itself
only reads kernel counters.

Ethernet magic-packet wake is a NetworkManager connection default. A saved
connection's explicit wake-on-LAN setting takes precedence. Changing this
default does not require disconnecting an active interface.

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
firewall and idle policy use TCP.

## Checks

```sh
nix flake check --no-build --no-update-lock-file
nix build --no-update-lock-file --max-jobs 1 --cores 4 \
  .#nixosConfigurations.predator.config.system.build.toplevel
python3 tests/night-suspend.py "$(nix eval --raw \
  .#nixosConfigurations.predator.config.systemd.services.night-suspend.serviceConfig.ExecStart)"
sudo systemctl --failed
sudo nextcloud-occ setupchecks
curl --fail https://photos.mirsella.mooo.com/api/server/ping
```

The sleep regression tests use fake devices, clocks, sessions and systemctl;
they never suspend the host. Flake checking without a build only evaluates
configuration. Changes to boot parameters and EFI permissions also need a
later boot or mount verification.
