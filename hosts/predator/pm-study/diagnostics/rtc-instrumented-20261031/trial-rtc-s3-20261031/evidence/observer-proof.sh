set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id
test "$(timedatectl show -p NTPSynchronized --value)" = yes
timedatectl timesync-status
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != "-" {print}')"
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
live=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
base=/home/mirsella/dev/predator-pm-debug/.agents-local/rtc-s3-20261031
grep -F 'RTC READY id=rtc-s3-20261031 nonce=rtc-s3-20261031-1793436900' "$live"
test ! -e "$base-baseline.log"
test ! -e "$base-result.log"
cp --no-clobber "$live" "$base-baseline.log"
sha256sum "$base-baseline.log"
