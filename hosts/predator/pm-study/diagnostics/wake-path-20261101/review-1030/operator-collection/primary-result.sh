set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=ns

cat /proc/sys/kernel/random/boot_id
cat /proc/uptime
grep '^btime ' /proc/stat
systemctl show predator-pm-path-path-d3-v2-20261101.service -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-jobs --no-legend
cat /sys/power/suspend_stats/{success,fail} /sys/power/{pm_test,pm_async,pm_trace,mem_sleep} /sys/module/pm_stage/parameters/{armed,markers,last_action,last_value,last_start}
cat /sys/class/rtc/rtc0/wakealarm
for f in /var/lib/predator-pm-wake-path/path-d3-v2-20261101-*.txt; do printf '\nFILE %s\n' "$f"; stat -c '%U %a' "$f"; cat "$f"; done
sha256sum /var/lib/predator-pm-wake-path/path-d3-v2-20261101-*
journalctl -b --no-pager -o short-iso-precise -u predator-pm-path-path-d3-v2-20261101.service

