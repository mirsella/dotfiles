set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
/nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0/bin/predator-pm-recorder-off check /var/lib/predator-pm-recorder-off/off-s3-20261101-plan.conf 9cbcb49aa9c5c61650874bbe8544af5b8241495f48368f4865c91ded9e957ba1
test ! -e /var/lib/predator-pm-recorder-off/off-s3-20261101-requested.txt
systemd-run --unit=predator-pm-recorder-off-off-s3-20261101 --on-calendar=@1793546100 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=900s --property=TimeoutStartSec=30s --property=TimeoutStopSec=180s --property='Environment=PATH=/run/current-system/sw/bin' --property='ExecStopPost=/nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0/bin/predator-pm-recorder-off restore /var/lib/predator-pm-recorder-off/off-s3-20261101-plan.conf 9cbcb49aa9c5c61650874bbe8544af5b8241495f48368f4865c91ded9e957ba1' /nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0/bin/predator-pm-recorder-off run /var/lib/predator-pm-recorder-off/off-s3-20261101-plan.conf 9cbcb49aa9c5c61650874bbe8544af5b8241495f48368f4865c91ded9e957ba1
systemctl show predator-pm-recorder-off-off-s3-20261101.service -p ActiveState -p MainPID -p ExecStart -p ExecStopPost -p Restart -p RuntimeMaxUSec
systemctl list-timers --all --no-pager predator-pm-recorder-off-off-s3-20261101.timer
test "$(systemctl show predator-pm-recorder-off-off-s3-20261101.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-recorder-off/off-s3-20261101-requested.txt
