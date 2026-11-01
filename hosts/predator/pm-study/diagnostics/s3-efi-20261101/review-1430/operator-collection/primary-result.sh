set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
cat /proc/sys/kernel/random/boot_id; grep '^btime ' /proc/stat
systemctl show predator-pm-boundary-s3-efi-v2-20261101.service systemd-suspend.service -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-jobs --no-pager
cat /sys/power/suspend_stats/{success,fail} /sys/power/pm_async /sys/power/pm_trace /sys/power/pm_test /sys/power/mem_sleep /sys/class/rtc/rtc0/wakealarm
stat -c '%U %a %s %n' /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-*
sha256sum /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-*
for f in /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-*.txt /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-*.json; do test -f "$f" || continue; echo "FILE $f"; cat "$f"; done
journalctl -u predator-pm-boundary-s3-efi-v2-20261101.service --no-pager -o short-iso-precise
