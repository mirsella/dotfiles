set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=ns

/nix/store/fgxkwdijv3z3n7iwkn7hv111xhydr8ax-predator-pm-study-0.1.0/bin/predator-pm-wake-path check /var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653
systemctl list-timers --all --no-pager nextcloud-cron.timer
test ! -e /var/lib/predator-pm-wake-path/path-d3-v2-20261101-requested.txt
systemd-run --unit=predator-pm-path-path-d3-v2-20261101 --on-calendar=@1793520900 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=30s --property=RuntimeMaxSec=2400s --property=TimeoutStopSec=180s --property=Environment=PATH=/run/current-system/sw/bin --property='ExecStopPost=/nix/store/fgxkwdijv3z3n7iwkn7hv111xhydr8ax-predator-pm-study-0.1.0/bin/predator-pm-wake-path restore /var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653' /nix/store/fgxkwdijv3z3n7iwkn7hv111xhydr8ax-predator-pm-study-0.1.0/bin/predator-pm-wake-path run /var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653
systemctl show predator-pm-path-path-d3-v2-20261101.service -p Type -p ActiveState -p RuntimeMaxUSec -p TimeoutStopUSec -p ExecStopPost -p Restart
systemctl list-timers --all --no-pager predator-pm-path-path-d3-v2-20261101.timer
test "$(systemctl show predator-pm-path-path-d3-v2-20261101.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-wake-path/path-d3-v2-20261101-requested.txt

