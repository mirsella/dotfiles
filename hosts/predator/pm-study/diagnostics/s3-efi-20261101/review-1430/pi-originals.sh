set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
tar -C /home/mirsella/dev/predator-pm-debug/.agents-local -cJf - s3-efi-v2-20261101-baseline.log s3-efi-v2-20261101-result.log
