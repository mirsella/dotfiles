set -euo pipefail
date --iso-8601=seconds
name=predator-pm-diagnostic-d-freezer-01-20261030
test "$(date +%s)" -lt 1793383112
systemctl is-active predator-netconsole-20261006.service
test "$(timedatectl show -p NTPSynchronized --value)" = yes
for suffix in probe snapshot; do test "$(systemctl show "$name-$suffix.timer" -p LoadState --value)" = not-found; done
test ! -e /home/mirsella/dev/predator-pm-debug/.agents-local/d-freezer-01-20261030-result.log
systemd-run --unit="$name-probe" --on-calendar=@1793383532 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=20s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19
systemd-run --unit="$name-snapshot" --on-calendar=@1793383652 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=30s /usr/bin/cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/d-freezer-01-20261030-result.log
systemctl cat "$name-probe.timer" "$name-probe.service" "$name-snapshot.timer" "$name-snapshot.service"
