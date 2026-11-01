set -euo pipefail
date --iso-8601=seconds
systemctl is-active predator-netconsole-20261006.service
test "$(timedatectl show -p NTPSynchronized --value)" = yes
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != "-" {print}')"
live=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
base=/home/mirsella/dev/predator-pm-debug/.agents-local/d-core-03-20261030
grep -F 'DIAG CAPTURE id=d-core-03-20261030 boot=f8eb6e44-85ae-4e56-bc00-587876699945 due=1793396012' "$live"
test ! -e "$base-baseline.log"
test ! -e "$base-result.log"
cp --no-clobber "$live" "$base-baseline.log"
stat -c '%s' "$base-baseline.log"
sha256sum "$base-baseline.log"
systemd-run --unit=predator-pm-diagnostic-d-core-03-20261030-probe --on-calendar=@1793396132 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=20s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19
systemd-run --unit=predator-pm-diagnostic-d-core-03-20261030-snapshot --on-calendar=@1793396252 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=30s /usr/bin/cp --no-clobber "$live" "$base-result.log"
systemd-run --unit=predator-pm-diagnostic-d-core-03-20261030-review --on-calendar=@1793396312 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=10s /usr/bin/printf 'DIAG REVIEW READY d-core-03-20261030\n'
systemctl list-timers --all --no-pager 'predator-pm-diagnostic-d-core-03-20261030*'
