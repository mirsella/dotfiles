set -euo pipefail
date --iso-8601=seconds
/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/d-processors-01-20261030-plan.conf eb7bab765ec157a0359693654513001a7ddbad35b515d4ee033e37fcb3679843
systemd-run --unit=predator-pm-diagnostic-d-processors-01-20261030 --on-calendar=@1793392712 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=180s --property=TimeoutStopSec=30s --property='ExecStopPost=/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic restore /var/lib/predator-pm-diagnostic/d-processors-01-20261030-plan.conf eb7bab765ec157a0359693654513001a7ddbad35b515d4ee033e37fcb3679843' --setenv=PATH=/run/current-system/sw/bin /nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic run /var/lib/predator-pm-diagnostic/d-processors-01-20261030-plan.conf eb7bab765ec157a0359693654513001a7ddbad35b515d4ee033e37fcb3679843
systemctl cat predator-pm-diagnostic-d-processors-01-20261030.timer predator-pm-diagnostic-d-processors-01-20261030.service
systemctl show predator-pm-diagnostic-d-processors-01-20261030.timer -p ActiveState -p SubState -p NextElapseUSecRealtime
systemctl show predator-pm-diagnostic-d-processors-01-20261030.service -p ActiveState -p MainPID -p Type -p RuntimeMaxUSec
test ! -e /var/lib/predator-pm-diagnostic/d-processors-01-20261030-requested.txt
cat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail /sys/power/pm_test /sys/power/pm_async
