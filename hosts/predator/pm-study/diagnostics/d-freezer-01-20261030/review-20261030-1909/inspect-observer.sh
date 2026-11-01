set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id
systemctl is-active predator-netconsole-20261006.service
timedatectl show -p NTPSynchronized
timedatectl timesync-status
name=predator-pm-diagnostic-d-freezer-01-20261030
journalctl -u "$name-probe.service" -u "$name-snapshot.service" --no-pager -o short-iso-precise
systemctl list-timers --all --no-pager 'predator-pm-*'
log=/home/mirsella/dev/predator-pm-debug/.agents-local/d-freezer-01-20261030-result.log
stat -c '%n %s %y' "$log" /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
sha256sum "$log" /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
ip neigh show 192.168.1.19
