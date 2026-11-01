set -euo pipefail
date --iso-8601=seconds
hostname
cat /proc/sys/kernel/random/boot_id /proc/uptime
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
for f in /sys/power/suspend_stats/{success,fail} /sys/module/pm_stage/parameters/{armed,markers} /sys/power/pm_async /sys/power/pm_trace; do printf '%s=' "$f"; cat "$f"; done
for f in /var/lib/predator-pm-study/r-serial-q35-20261030-*; do stat -c '%n %s %y' "$f"; sha256sum "$f"; done
cat /var/lib/predator-pm-study/last-completed.txt
for suffix in kernel-completed completed; do
  f="/var/lib/predator-pm-study/r-serial-q35-20261030-$suffix.txt"
  if test -e "$f"; then cat "$f"; else printf 'ABSENT=%s\n' "$f"; fi
done
systemctl show predator-pm-r-serial-q35-20261030.service -p ActiveState -p SubState -p Result -p ExecMainStatus
systemctl --failed --no-pager
systemctl list-jobs --no-pager
systemctl list-timers --all --no-pager 'predator-pm-*' 'night-suspend*'
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman hermes-agent hermes-backend systemd-timesyncd
docker ps --format '{{.Names}} {{.Status}}'
timedatectl timesync-status
journalctl -b 8b7a173d84064072aebcc85c8b4989e7 -u systemd-timesyncd --since '2026-10-30 12:34:00' --no-pager -o short-iso-precise
zpool status -x
findmnt -rn -t zfs -o SOURCE,TARGET
cd /home/mirsella/dev/dotfiles
sha256sum hosts/predator/pm-study/README.md hosts/predator/pm-study/trials/r-serial-q35-20261030/README.md pkgs/predator-pm-study/{main,trial}.rs
git status --short
git diff -- flake.lock
