set -euo pipefail
date --iso-8601=seconds
hostname
boot=3c1873e4-b866-41e1-b411-702168ef90cf
test "$(cat /proc/sys/kernel/random/boot_id)" = "$boot"
cat /proc/sys/kernel/random/boot_id
grep '^btime ' /proc/stat
cat /proc/uptime
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
sha256sum /nix/store/5gqrh1r71xnl127sj8dv83nnlk1nf6vs-predator-pm-study-0.1.0/bin/predator-pm-{study,control}
for f in /sys/power/suspend_stats/{success,fail} /sys/module/pm_stage/parameters/{armed,markers} /sys/power/{pm_async,pm_trace}; do printf '%s=' "$f"; cat "$f"; done
systemctl list-jobs --no-pager
systemctl --failed --no-pager
systemctl show predator-pm-r-serial-q35r1-20261030.service -p LoadState -p ActiveState -p SubState -p Result -p ExecMainStartTimestamp -p ExecMainExitTimestamp -p ExecMainStatus -p MainPID -p ControlPID -p ControlGroup
systemctl list-timers --all --no-pager 'predator-pm-*' 'night-suspend*' nextcloud-cron.timer
systemctl cat nextcloud-cron.service nextcloud-cron.timer
systemctl show nextcloud-cron.service -p LoadState -p ActiveState -p SubState -p Result -p ExecCondition -p ExecStart -p ExecMainStartTimestamp -p ExecMainExitTimestamp -p ExecMainStatus
journalctl -b "$boot" -u nextcloud-cron.service -u nextcloud-cron.timer --since '2026-10-30 14:10:00' --until '2026-10-30 14:15:00' --no-pager -o short-iso-precise
journalctl -b "$boot" -u systemd-suspend.service --no-pager -o short-iso-precise
dir=/var/lib/predator-pm-study
for file in "$dir"/r-serial-q35r1-20261030-* "$dir/last-completed.txt"; do
  stat -c '%n %s %y' "$file"
  sha256sum "$file"
done
for suffix in return after-kernel after-service verification-kernel verification kernel-completed completed; do
  file="$dir/r-serial-q35r1-20261030-$suffix.txt"
  if test -e "$file"; then printf 'PRESENT %s\n' "$file"; else printf 'ABSENT %s\n' "$file"; fi
done
cat "$dir/last-completed.txt"
timedatectl show -p NTPSynchronized --value
timedatectl timesync-status
journalctl -b "$boot" -u systemd-timesyncd --no-pager -o short-iso-precise
zpool status
findmnt -rn -t zfs -o SOURCE,TARGET
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
docker ps --format '{{.Names}} {{.Status}}'
ps -eo pid,ppid,etimes,args
cd /home/mirsella/dev/dotfiles
sudo -u mirsella git status --short
sudo -u mirsella git diff -- flake.lock
sha256sum hosts/predator/pm-study/README.md hosts/predator/pm-study/trials/r-serial-q35r1-20261030/README.md pkgs/predator-pm-study/trial.rs
