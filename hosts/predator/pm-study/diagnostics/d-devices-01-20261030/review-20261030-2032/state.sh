set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id /proc/uptime
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
sha256sum /nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic /var/lib/predator-pm-diagnostic/d-devices-01-20261030-* /var/lib/predator-pm-study/last-completed.txt
stat -c '%n %u %a %s' /var/lib/predator-pm-diagnostic/d-devices-01-20261030-*
for f in /sys/power/suspend_stats/{success,fail} /sys/power/{pm_test,pm_async,pm_trace} /sys/module/pm_stage/parameters/{armed,markers}; do printf '%s=' "$f"; cat "$f"; done
systemctl show systemd-suspend.service predator-pm-diagnostic-d-devices-01-20261030.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-jobs --no-pager
systemctl --failed --no-pager
nmcli -g GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
timedatectl timesync-status
zpool status
for m in /srv/data/fast /var/lib/nextcloud/data /srv/backup /srv/backup/recovery /srv/data/archive; do findmnt -n -o SOURCE,TARGET -M "$m"; done
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
podman ps --format '{{.Names}} {{.Status}}'
systemctl list-timers --all --no-pager 'predator-pm-*' nextcloud-cron.timer night-suspend.timer
journalctl -b f8eb6e4485ae4e56bc00587876699945 -u predator-pm-diagnostic-d-devices-01-20261030.service -u systemd-suspend.service --no-pager -o short-iso-precise
