set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
date --iso-8601=ns
test $(date +%s) -lt $((1793539200-300))
test "$(timedatectl show -p NTPSynchronized --value)" = yes
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
systemd-run --unit=predator-pm-boundary-s3-efi-v2-20261101-probe --on-calendar=@1793539590 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=30s --property=TimeoutStartSec=20s /bin/bash -c 'date --iso-8601=ns; timeout 15s ssh-keyscan -T 7 -t ed25519 192.168.1.19; date --iso-8601=ns'
systemd-run --unit=predator-pm-boundary-s3-efi-v2-20261101-snapshot --on-calendar=@1793539680 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=30s --property=TimeoutStartSec=20s /bin/cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/s3-efi-v2-20261101-result.log
systemctl list-timers --all --no-pager 'predator-pm-boundary-s3-efi-v2-20261101-*'
