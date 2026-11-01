set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
date --iso-8601=ns
cat /proc/sys/kernel/random/boot_id
timedatectl
timedatectl timesync-status
systemctl show predator-netconsole-20261006.service -p ActiveState -p MainPID
systemctl list-jobs --no-pager
systemctl list-timers --all --no-pager 'predator-pm-*'
sha256sum /home/mirsella/wol_predator.py
sha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/s3-efi-v2-20261101-{baseline,result}.log
date --iso-8601=ns
