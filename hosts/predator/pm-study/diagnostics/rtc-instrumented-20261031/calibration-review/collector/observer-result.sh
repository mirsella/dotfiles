set -euo pipefail
date --iso-8601=seconds
sha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/rtc-cal-core-r3-20261031-baseline.log /home/mirsella/dev/predator-pm-debug/.agents-local/rtc-cal-core-r3-20261031-result.log
cat /home/mirsella/dev/predator-pm-debug/.agents-local/rtc-cal-core-r3-20261031-result.log
journalctl -b --no-pager -o short-iso-precise -u predator-pm-instrumented-rtc-cal-core-r3-20261031-probe.service -u predator-pm-instrumented-rtc-cal-core-r3-20261031-snapshot.service -u predator-pm-instrumented-rtc-cal-core-r3-20261031-wake.service -u predator-pm-instrumented-rtc-cal-core-r3-20261031-wake-retry.service
systemctl list-timers --all --no-pager 'predator-pm-instrumented-rtc-cal-core-r3-20261031*'
