set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
/nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3 check /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-plan.conf bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c
test ! -e /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-requested.txt
systemd-run --unit=predator-pm-boundary-s3-efi-v2-20261101 --on-calendar=@1793539200 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=900s --property=TimeoutStartSec=30s --property=TimeoutStopSec=180s --property='Environment=PATH=/run/current-system/sw/bin' --property='ExecStopPost=/nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3 restore /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-plan.conf bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c' /nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3 run /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-plan.conf bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c
systemctl show predator-pm-boundary-s3-efi-v2-20261101.service -p ActiveState -p MainPID -p ExecStart -p ExecStopPost -p Restart -p RuntimeMaxUSec
systemctl list-timers --all --no-pager predator-pm-boundary-s3-efi-v2-20261101.timer
test "$(systemctl show predator-pm-boundary-s3-efi-v2-20261101.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-requested.txt
