set -euo pipefail
date --iso-8601=seconds
name=predator-pm-diagnostic-d-freezer-01-20261030
runtime=/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
plan=/var/lib/predator-pm-diagnostic/d-freezer-01-20261030-plan.conf
hash=9f2e4cbe873f8f5c5a8ead91f707deafcda8d7d260228cec54c7a43e6590c874
test "$(systemctl show "$name.timer" -p LoadState --value)" = not-found
test "$(systemctl show "$name.service" -p LoadState --value)" = not-found
"$runtime" check "$plan" "$hash"
systemd-run --unit="$name" --on-calendar=@1793383412 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=180s --property="ExecStopPost=$runtime restore $plan $hash" --setenv=PATH=/run/current-system/sw/bin "$runtime" run "$plan" "$hash"
systemctl cat "$name.timer" "$name.service"
systemctl show "$name.timer" -p ActiveState -p SubState -p NextElapseUSecRealtime
systemctl show "$name.service" -p ActiveState -p MainPID
for file in /sys/power/suspend_stats/{success,fail} /sys/power/{pm_async,pm_test} /sys/module/pm_stage/parameters/{armed,markers}; do printf '%s=' "$file"; cat "$file"; done
test ! -e /var/lib/predator-pm-diagnostic/d-freezer-01-20261030-requested.txt
