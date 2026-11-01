set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
date --iso-8601=ns

sha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/path-d3-v2-20261101-baseline.log /home/mirsella/dev/predator-pm-debug/.agents-local/path-d3-v2-20261101-result.log
cat /home/mirsella/dev/predator-pm-debug/.agents-local/path-d3-v2-20261101-result.log
journalctl -b --no-pager -o short-iso-precise -u predator-pm-path-path-d3-v2-20261101-probe.service -u predator-pm-path-path-d3-v2-20261101-snapshot.service -u predator-pm-path-path-d3-v2-20261101-wake.service -u predator-pm-path-path-d3-v2-20261101-wake-retry.service
systemctl list-timers --all --no-pager 'predator-pm-path-path-d3-v2-20261101*'

