set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id
cat /proc/uptime
grep '^btime ' /proc/stat
systemctl show predator-pm-wake-wake-20261031.service -p ActiveState -p Result -p ExecMainStatus
systemctl list-jobs --no-legend
cat /sys/power/suspend_stats/{success,fail} /sys/power/{pm_test,pm_async,pm_trace,mem_sleep} /sys/module/pm_stage/parameters/{armed,markers,last_action,last_value,last_start}
cat /sys/class/rtc/rtc0/wakealarm
for f in /var/lib/predator-pm-wake-validation/wake-20261031-*.txt; do printf '\nFILE %s\n' "$f"; stat -c '%U %a' "$f"; cat "$f"; done
sha256sum /var/lib/predator-pm-wake-validation/wake-20261031-*
journalctl -b --no-pager -o short-iso-precise -u predator-pm-wake-wake-20261031.service
