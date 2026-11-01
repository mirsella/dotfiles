set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H); test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=seconds
for p in gpe69 gpe66 gpe6D; do printf '%s=' "$p"; cat "/sys/firmware/acpi/interrupts/$p"; done
cat /sys/bus/pci/devices/0000:00:1c.3/firmware_node/path
cat /sys/bus/pci/devices/0000:03:00.0/firmware_node/path
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
cat /sys/power/suspend_stats/{success,fail}
