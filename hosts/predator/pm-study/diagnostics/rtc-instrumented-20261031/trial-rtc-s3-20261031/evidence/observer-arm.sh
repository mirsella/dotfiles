set -euo pipefail
date --iso-8601=seconds
test "$(date +%s)" -lt 1793436600
test "$(timedatectl show -p NTPSynchronized --value)" = yes
systemctl is-active predator-netconsole-20261006.service
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
systemd-run --unit=predator-pm-instrumented-rtc-s3-20261031-wake --on-calendar=@1793437860 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/python3 /home/mirsella/wol_predator.py
systemd-run --unit=predator-pm-instrumented-rtc-s3-20261031-wake-retry --on-calendar=@1793437890 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/python3 /home/mirsella/wol_predator.py
systemd-run --unit=predator-pm-instrumented-rtc-s3-20261031-probe --on-calendar=@1793437950 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19
systemd-run --unit=predator-pm-instrumented-rtc-s3-20261031-snapshot --on-calendar=@1793438040 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s /usr/bin/cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/rtc-s3-20261031-result.log
systemctl list-timers --all --no-pager 'predator-pm-instrumented-rtc-s3-20261031*'
