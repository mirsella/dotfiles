set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id /proc/uptime
grep '^btime ' /proc/stat
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
sha256sum /nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
for f in /sys/power/suspend_stats/{success,fail} /sys/power/{pm_async,pm_test,pm_trace,mem_sleep} /sys/module/pm_stage/parameters/{armed,markers}; do printf '%s=' "$f"; cat "$f"; done
systemctl show predator-pm-diagnostic-d-freezer-01-20261030.service systemd-suspend.service -p Id -p ActiveState -p SubState -p Result -p ExecMainStatus -p MainPID -p ControlPID -p ControlGroup -p ExecMainStartTimestamp -p ExecMainExitTimestamp
systemctl list-jobs --no-pager
systemctl --failed --no-pager
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer nextcloud-cron.timer
systemctl show nextcloud-cron.timer -p TimersMonotonic -p NextElapseUSecMonotonic -p LastTriggerUSec -p AccuracyUSec -p RandomizedDelayUSec
find /var/lib/predator-pm-diagnostic -maxdepth 1 -name 'd-freezer-01-20261030-*' -type f -printf '%f %s %TY-%Tm-%TdT%TH:%TM:%TS %u %m\n'
sha256sum /var/lib/predator-pm-diagnostic/d-freezer-01-20261030-* /var/lib/predator-pm-study/last-completed.txt
for f in /var/lib/predator-pm-diagnostic/d-freezer-01-20261030-{return,outcome,completed}.txt /var/lib/predator-pm-study/last-completed.txt; do printf '\nFILE %s\n' "$f"; if test -f "$f"; then cat "$f"; else echo ABSENT; fi; done
journalctl -b f8eb6e4485ae4e56bc00587876699945 -u predator-pm-diagnostic-d-freezer-01-20261030.service -u systemd-suspend.service --no-pager -o short-iso-precise
journalctl -b f8eb6e4485ae4e56bc00587876699945 -u nextcloud-cron.service --since '2026-10-30 18:55:00' --no-pager -o short-iso-precise
timedatectl
timedatectl timesync-status
zpool status
for m in /srv/data/fast /var/lib/nextcloud/data /srv/backup /srv/backup/recovery /srv/data/archive; do findmnt -n -o SOURCE,TARGET -M "$m"; done
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
podman ps --format '{{.Names}} {{.Status}}'
ps -eo pid,ppid,stat,comm,args
sha256sum /home/mirsella/dev/dotfiles/hosts/predator/pm-study/README.md /home/mirsella/dev/dotfiles/hosts/predator/pm-study/diagnostics/README.md /home/mirsella/dev/dotfiles/hosts/predator/pm-study/diagnostics/d-freezer-01-20261030/README.md
