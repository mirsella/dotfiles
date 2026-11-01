set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
date --iso-8601=ns

test "$(date +%s)" -lt 1793520600
test "$(timedatectl show -p NTPSynchronized --value)" = yes
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
systemd-run --unit=predator-pm-path-path-d3-v2-20261101-wake --on-calendar=@1793521860 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/python3 /home/mirsella/wol_predator.py
systemd-run --unit=predator-pm-path-path-d3-v2-20261101-wake-retry --on-calendar=@1793521890 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/python3 /home/mirsella/wol_predator.py
systemd-run --unit=predator-pm-path-path-d3-v2-20261101-probe --on-calendar=@1793522190 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19
systemd-run --unit=predator-pm-path-path-d3-v2-20261101-snapshot --on-calendar=@1793522280 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/path-d3-v2-20261101-result.log
systemctl list-timers --all --no-pager 'predator-pm-path-path-d3-v2-20261101*'

