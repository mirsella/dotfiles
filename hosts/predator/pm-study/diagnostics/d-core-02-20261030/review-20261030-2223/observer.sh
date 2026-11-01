set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id
systemctl is-active predator-netconsole-20261006.service
timedatectl show -p NTPSynchronized
journalctl -u predator-pm-diagnostic-d-core-02-20261030-probe.service -u predator-pm-diagnostic-d-core-02-20261030-snapshot.service -u predator-pm-diagnostic-d-core-02-20261030-review.service --no-pager -o short-iso-precise
stat -c '%s' /home/mirsella/dev/predator-pm-debug/.agents-local/d-core-02-20261030-result.log
sha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/d-core-02-20261030-result.log
systemctl list-timers --all --no-pager 'predator-pm-*'
