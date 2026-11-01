set -euo pipefail
date --iso-8601=seconds
test "$(date +%s)" -lt 1793384312
systemctl is-active predator-netconsole-20261006.service
test "$(timedatectl show -p NTPSynchronized --value)" = yes
live=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
baseline=/home/mirsella/dev/predator-pm-debug/.agents-local/d-freezer-02-20261030-baseline.log
result=/home/mirsella/dev/predator-pm-debug/.agents-local/d-freezer-02-20261030-result.log
grep -F 'DIAG CAPTURE id=d-freezer-02-20261030 boot=f8eb6e44-85ae-4e56-bc00-587876699945 nonce=1793383960' "$live"
test ! -e "$baseline"
test ! -e "$result"
cp --no-clobber "$live" "$baseline"
stat -c '%n %s %y' "$baseline"
sha256sum "$baseline"
name=predator-pm-diagnostic-d-freezer-02-20261030
for suffix in probe snapshot; do test "$(systemctl show "$name-$suffix.timer" -p LoadState --value)" = not-found; done
systemd-run --unit="$name-probe" --on-calendar=@1793384732 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=20s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19
systemd-run --unit="$name-snapshot" --on-calendar=@1793384852 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=30s /usr/bin/cp --no-clobber "$live" "$result"
systemctl cat "$name-probe.timer" "$name-probe.service" "$name-snapshot.timer" "$name-snapshot.service"
