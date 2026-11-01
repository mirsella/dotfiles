set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
/nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial check /var/lib/predator-pm-efi-s3/core-efi-20261101-plan.conf 95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3
test ! -e /var/lib/predator-pm-efi-s3/core-efi-20261101-requested.txt
systemd-run --unit=predator-pm-boundary-core-efi-20261101 --on-calendar=@1793537100 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=900s --property=TimeoutStartSec=30s --property=TimeoutStopSec=180s --property='Environment=PATH=/run/current-system/sw/bin' --property='ExecStopPost=/nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial restore /var/lib/predator-pm-efi-s3/core-efi-20261101-plan.conf 95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3' /nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial run /var/lib/predator-pm-efi-s3/core-efi-20261101-plan.conf 95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3
systemctl show predator-pm-boundary-core-efi-20261101.service -p ActiveState -p MainPID -p ExecStart -p ExecStopPost -p Restart -p RuntimeMaxUSec
systemctl list-timers --all --no-pager predator-pm-boundary-core-efi-20261101.timer
test "$(systemctl show predator-pm-boundary-core-efi-20261101.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-efi-s3/core-efi-20261101-requested.txt
