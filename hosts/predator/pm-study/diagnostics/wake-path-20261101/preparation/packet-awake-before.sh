set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
date --iso-8601=ns
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
systemctl is-system-running
systemctl show systemd-suspend.service -p ActiveState -p MainPID
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend --plain)"
test "$(cat /sys/power/suspend_stats/success)" = 1
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/class/net/enp3s0f1/carrier)" = 1
printf 'awake_guard=PASS label=awake-packet-20261101-0835\n'
date --iso-8601=ns
