set -euo pipefail
date --iso-8601=seconds
/nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented check /var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-plan.conf d9994805b88f3a30454407f32d75e1d2bbaa08b975b9e86af1380b00b5e8b8b7
systemctl list-timers --all --no-pager nextcloud-cron.timer
test ! -e /var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-requested.txt
systemd-run --unit=predator-pm-instrumented-rtc-cal-core-r3-20261031 --on-calendar=@1793435700 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=2400s --property=TimeoutStopSec=180s --property=Environment=PATH=/run/current-system/sw/bin --property='ExecStopPost=/nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented restore /var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-plan.conf d9994805b88f3a30454407f32d75e1d2bbaa08b975b9e86af1380b00b5e8b8b7' /nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented run /var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-plan.conf d9994805b88f3a30454407f32d75e1d2bbaa08b975b9e86af1380b00b5e8b8b7
systemctl show predator-pm-instrumented-rtc-cal-core-r3-20261031.service -p Type -p ActiveState -p RuntimeMaxUSec -p TimeoutStopUSec -p ExecStopPost
systemctl list-timers --all --no-pager predator-pm-instrumented-rtc-cal-core-r3-20261031.timer
test "$(systemctl show predator-pm-instrumented-rtc-cal-core-r3-20261031.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-requested.txt
