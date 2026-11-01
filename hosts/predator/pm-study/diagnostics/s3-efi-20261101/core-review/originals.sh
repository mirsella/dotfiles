set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
test "$(systemctl show systemd-suspend -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend -p MainPID --value)" = 0
test ! -e /sys/module/pm_boundary_stage
tar -C /var/lib -cJf - predator-pm-efi-s3
