set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = bf51c3ac-164e-4bfa-b56c-7b71df526b7f
date --iso-8601=ns
systemctl show systemd-suspend.service -p ActiveState -p MainPID -p Result
test ! -d /sys/module/pm_boundary_compact
cat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail
dir=/var/lib/predator-pm-boundary/compact-20261101-a
find "$dir" -maxdepth 1 -type f -printf '%u %m %s %f\n'
sha256sum "$dir"/*
for name in request.txt before.txt load-result.txt status-before.txt write-result.txt status-after.txt readback.txt readback-error.txt after-write.txt outcome.txt; do
  if test -f "$dir/$name"; then printf '\nfile=%s\n' "$name"; cat "$dir/$name"; else printf 'not present: %s\n' "$name"; fi
done
df -B1 /boot /sys/firmware/efi/efivars
timedatectl show -p NTPSynchronized -p LocalRTC
hwclock --show --utc
journalctl -b bf51c3ac164e4bfab56c7b71df526b7f -k --since '2026-11-01 12:27:00' --no-pager -o short-iso-precise
systemctl is-system-running
systemctl list-jobs --no-legend
