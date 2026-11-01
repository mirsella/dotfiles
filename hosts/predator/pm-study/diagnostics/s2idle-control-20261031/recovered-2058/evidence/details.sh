set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
for f in /var/lib/predator-pm-s2idle/s2idle-20261031-{return,raw-return,clock-quiescence,request-ok}.txt; do
  printf '\nFILE %s\n' "$f"; cat "$f"
done
printf '\nOLD KERNEL\n'
journalctl -b ae29f67591674975a88401885468bc38 -k --no-pager -o short-monotonic
printf '\nOLD SERVICES\n'
journalctl -b ae29f67591674975a88401885468bc38 --no-pager -o short-monotonic -u predator-pm-s2idle-s2idle-20261031.service -u systemd-suspend.service -u NetworkManager.service -u systemd-timesyncd.service
printf '\nCURRENT HANDOFF\n'
journalctl -b --no-pager -o short-monotonic -u predator-pm-preserve-rtc.service -u predator-pm-clock-handoff.service -u systemd-timesyncd.service -u NetworkManager.service
systemctl show predator-pm-clock-handoff.service -p ActiveState -p Result -p ExecMainStatus
timedatectl status
timedatectl timesync-status
nmcli -g GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
printf '\nWAKE CONFIG\n'
for device in 0000:03:00.1 0000:03:00.0 0000:00:1c.3; do
  for field in wakeup control runtime_status; do
    f=/sys/bus/pci/devices/$device/power/$field
    test ! -f "$f" || { printf '%s=' "$f"; cat "$f"; }
  done
done
for f in /sys/class/rtc/rtc*/name /sys/class/rtc/rtc*/device/power/wakeup /sys/class/rtc/rtc*/wakealarm /sys/power/wakeup_count /sys/power/pm_wakeup_irq; do
  test ! -f "$f" || { printf '%s=' "$f"; cat "$f"; }
done
cat /sys/kernel/debug/wakeup_sources
