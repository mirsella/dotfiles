set -euo pipefail
date --iso-8601=seconds
name=predator-pm-diagnostic-d-freezer-02-20261030
runtime=/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
plan=/var/lib/predator-pm-diagnostic/d-freezer-02-20261030-plan.conf
hash=91e1616b734ee77a0077bccd130cecfe5621dfa0fba8b2326207e1064a8f4484
test "$(systemctl show "$name.timer" -p LoadState --value)" = not-found
test "$(systemctl show "$name.service" -p LoadState --value)" = not-found
"$runtime" check "$plan" "$hash"
systemd-run --unit="$name" --on-calendar=@1793384612 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=180s --property=TimeoutStopSec=30s --property="ExecStopPost=$runtime restore $plan $hash" --setenv=PATH=/run/current-system/sw/bin "$runtime" run "$plan" "$hash"
systemctl cat "$name.timer" "$name.service"
systemctl show "$name.timer" -p ActiveState -p SubState -p NextElapseUSecRealtime
systemctl show "$name.service" -p ActiveState -p MainPID -p Type -p RuntimeMaxUSec -p TimeoutStartUSec -p TimeoutStopUSec
for f in /sys/power/suspend_stats/{success,fail} /sys/power/{pm_async,pm_test} /sys/module/pm_stage/parameters/{armed,markers}; do printf '%s=' "$f"; cat "$f"; done
test ! -e /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-requested.txt
