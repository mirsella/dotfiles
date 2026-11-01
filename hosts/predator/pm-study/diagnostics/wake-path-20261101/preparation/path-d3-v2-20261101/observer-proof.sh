set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
date --iso-8601=ns

test "$(timedatectl show -p NTPSynchronized --value)" = yes
timedatectl timesync-status
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != "-" {print}')"
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
live=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
base=/home/mirsella/dev/predator-pm-debug/.agents-local/path-d3-v2-20261101
grep -F 'WAKEPATH READY id=path-d3-v2-20261101 nonce=path-d3-v2-20261101-1793520900' "$live"
test ! -e "$base-baseline.log"
test ! -e "$base-result.log"
cp --no-clobber "$live" "$base-baseline.log"
sha256sum "$base-baseline.log"

