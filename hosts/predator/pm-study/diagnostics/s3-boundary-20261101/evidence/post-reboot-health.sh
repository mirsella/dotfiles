set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
test "$(TZ=Europe/Paris date +%H)" -ge 8
test "$(TZ=Europe/Paris date +%H)" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = bf51c3ac-164e-4bfa-b56c-7b71df526b7f
timeout 120s systemctl is-system-running --wait
set -euo pipefail
date --iso-8601=seconds
boot=$(cat /proc/sys/kernel/random/boot_id)
compact=${boot//-/}
printf 'boot=%s\n' "$boot"
cat /proc/uptime
grep '^btime ' /proc/stat
for p in /run/current-system /run/booted-system /run/current-system/kernel /run/current-system/kernel-modules; do readlink -e "$p"; done
systemctl is-system-running
systemctl list-jobs --no-pager
systemctl --failed --no-pager
for p in /sys/power/suspend_stats/success /sys/power/suspend_stats/fail /sys/power/pm_async /sys/power/pm_trace /sys/power/pm_test /sys/power/mem_sleep /sys/module/pm_stage/parameters/armed /sys/module/pm_stage/parameters/markers /sys/module/pm_stage/parameters/last_action /sys/module/pm_stage/parameters/last_value /sys/module/pm_stage/parameters/last_start /sys/class/rtc/rtc0/wakealarm /sys/class/power_supply/ACAD/online /sys/class/graphics/fb0/blank; do printf '%s=' "$p"; cat "$p"; done
test ! -e /var/lib/predator-pm-study/HOLD
test ! -e /var/lib/predator-pm-diagnostic/HOLD
predator-boot audit
cat /proc/acpi/button/lid/LID0/state
ethtool enp3s0f1
nmcli -g GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
timedatectl status
timedatectl timesync-status
zpool status
for p in /srv/data/fast /var/lib/nextcloud/data /srv/backup /srv/backup/recovery /srv/data/archive; do findmnt -n -o SOURCE,TARGET --mountpoint "$p"; done
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
podman ps --format '{{.Names}} {{.Status}}'
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer nextcloud-cron.timer
systemctl list-units --no-legend --state=active,activating,reloading,deactivating db-backup.service recovery-backup.service hermes-backup.service restic-backup.service sanoid.service 'syncoid-*.service' 'zfs-scrub*.service' 'zpool-trim*.service' nixos-upgrade.service nextcloud-setup.service nextcloud-cron.service
if pgrep -a -G nixbld; then exit 1; else test "$?" = 1; fi
systemctl cat predator-pm-preserve-rtc.service predator-pm-clock-handoff.service systemd-timesyncd.service
systemctl show predator-pm-preserve-rtc.service predator-pm-clock-handoff.service systemd-timesyncd.service -p Id -p ActiveState -p Result -p ExecMainStartTimestamp -p ExecMainExitTimestamp -p Before -p After
journalctl -b "$compact" -u predator-pm-preserve-rtc.service -u predator-pm-clock-handoff.service -u systemd-timesyncd.service --no-pager -o short-iso-precise
stat -c '%n %s %U %a %y' "/var/lib/predator-pm-study/$boot-boot-rtc.txt"
head -n 5 "/var/lib/predator-pm-study/$boot-boot-rtc.txt"
hwclock --show --utc
modinfo pm_stage
sha256sum /nix/store/cs0lilrdblqkd20835ihmlk61yf8nh0b-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/ladder-summary-20261030/README.md pkgs/predator-pm-study/diagnostic.rs pkgs/predator-pm-study/pm_stage.c
