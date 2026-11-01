set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
cat /proc/sys/kernel/random/boot_id /proc/uptime
grep '^btime ' /proc/stat
systemctl show systemd-suspend.service predator-pm-boundary-core-efi-20261101.service -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-jobs --no-pager
cat /sys/power/suspend_stats/{success,fail} /sys/power/{pm_async,pm_trace,pm_test,mem_sleep} /sys/class/rtc/rtc0/wakealarm
for n in nonce boot_id mode armed status readback; do printf 'STAGE %s\n' "$n"; cat "/sys/module/pm_boundary_stage/parameters/$n"; done
timedatectl show -p NTPSynchronized -p LocalRTC
hwclock --show --utc
cat /sys/class/rtc/rtc0/since_epoch
date +%s
stat -c '%U %a %s %n' /var/lib/predator-pm-efi-s3/core-efi-20261101-*
sha256sum /var/lib/predator-pm-efi-s3/core-efi-20261101-*
for n in outcome.txt raw-return.txt restored.txt completed.json; do p=/var/lib/predator-pm-efi-s3/core-efi-20261101-$n; if test -f "$p"; then printf 'FILE %s\n' "$p"; cat "$p"; else printf 'absent after successful listing: %s\n' "$p"; fi; done
journalctl -b 39d7579bb86d4d30b910131943e550c8 -k --since '2026-11-01 13:44:00' --no-pager -o short-iso-precise
