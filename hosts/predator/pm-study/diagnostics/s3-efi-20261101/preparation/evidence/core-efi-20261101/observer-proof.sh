set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
date --iso-8601=ns
test "$(timedatectl show -p NTPSynchronized --value)" = yes
timedatectl timesync-status
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
grep -F 'BOUNDARY READY id=core-efi-20261101 nonce=core1101a boot=39d7579b-b86d-4d30-b910-131943e550c8' /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log | tail -1
test ! -e /home/mirsella/dev/predator-pm-debug/.agents-local/core-efi-20261101-baseline.log; test ! -e /home/mirsella/dev/predator-pm-debug/.agents-local/core-efi-20261101-result.log
cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/core-efi-20261101-baseline.log
sha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/core-efi-20261101-baseline.log
sha256sum /home/mirsella/wol_predator.py

