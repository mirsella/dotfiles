set -euo pipefail
date --iso-8601=seconds
/nix/store/cs0lilrdblqkd20835ihmlk61yf8nh0b-predator-pm-study-0.1.0/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/d-core-02-20261030-plan.conf eede96ef0db1a75f2cf7b4a3c92900608bbf3d5df92b8e59955d4361a36716b3
systemd-run --unit=predator-pm-diagnostic-d-core-02-20261030 --on-calendar=@1793395112 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=180s --property=TimeoutStopSec=30s --property='ExecStopPost=/nix/store/cs0lilrdblqkd20835ihmlk61yf8nh0b-predator-pm-study-0.1.0/bin/predator-pm-diagnostic restore /var/lib/predator-pm-diagnostic/d-core-02-20261030-plan.conf eede96ef0db1a75f2cf7b4a3c92900608bbf3d5df92b8e59955d4361a36716b3' --setenv=PATH=/run/current-system/sw/bin /nix/store/cs0lilrdblqkd20835ihmlk61yf8nh0b-predator-pm-study-0.1.0/bin/predator-pm-diagnostic run /var/lib/predator-pm-diagnostic/d-core-02-20261030-plan.conf eede96ef0db1a75f2cf7b4a3c92900608bbf3d5df92b8e59955d4361a36716b3
systemctl cat predator-pm-diagnostic-d-core-02-20261030.timer predator-pm-diagnostic-d-core-02-20261030.service
systemctl show predator-pm-diagnostic-d-core-02-20261030.timer -p ActiveState -p SubState -p NextElapseUSecRealtime
systemctl show predator-pm-diagnostic-d-core-02-20261030.service -p ActiveState -p MainPID -p Type -p RuntimeMaxUSec
test ! -e /var/lib/predator-pm-diagnostic/d-core-02-20261030-requested.txt
cat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail /sys/power/pm_test /sys/power/pm_async
