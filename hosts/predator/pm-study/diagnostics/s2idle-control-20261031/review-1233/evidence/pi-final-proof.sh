set -euo pipefail
date --iso-8601=seconds
cat /proc/sys/kernel/random/boot_id
timedatectl show -p NTPSynchronized
timedatectl timesync-status
systemctl show predator-netconsole-20261006.service -p ActiveState -p SubState -p MainPID -p ActiveEnterTimestamp
systemctl list-jobs --no-pager
systemctl list-timers --all --no-pager 'predator-pm-*'
sha256sum /home/mirsella/wol_predator.py /home/mirsella/dev/predator-pm-debug/.agents-local/s2idle-20261031-{baseline,result}.log
stat -c '%n %U %a %s %y' /home/mirsella/dev/predator-pm-debug/.agents-local/s2idle-20261031-{baseline,result}.log
for suffix in wake wake-retry probe snapshot; do
  printf 'UNIT %s\n' "$suffix"
  systemctl show "predator-pm-s2idle-s2idle-20261031-$suffix.service" -p ActiveState -p Result -p ExecMainStatus -p ExecMainStartTimestamp -p ExecMainExitTimestamp
done
