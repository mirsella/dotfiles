set -euo pipefail
date --iso-8601=seconds
boot=$(cat /proc/sys/kernel/random/boot_id)
printf 'BOOT %s\n' "$boot"
cat /proc/uptime
grep '^btime ' /proc/stat
readlink -f /run/current-system /run/booted-system /run/current-system/kernel /run/current-system/kernel-modules
rtc=/var/lib/predator-pm-study/$boot-boot-rtc.txt
stat -c '%n %U %a %s %y' "$rtc"
sha256sum "$rtc"
cat "$rtc"
printf '\nOLD TRIAL INVENTORY\n'
find /var/lib/predator-pm-s2idle -maxdepth 1 -type f -name 's2idle-20261031-*' -printf '%f %u %m %s\n'
sha256sum /var/lib/predator-pm-s2idle/s2idle-20261031-*
printf '\nCURRENT STATE\n'
systemctl list-jobs --no-pager
systemctl --failed --no-pager
cat /sys/power/suspend_stats/{success,fail} /sys/power/{pm_test,pm_async,pm_trace,mem_sleep} /sys/module/pm_stage/parameters/{armed,markers}
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer nextcloud-cron.timer
printf '\nWAKE ROUTES\n'
cat /proc/acpi/wakeup
for device in 0000:03:00.1 0000:03:00.0 0000:00:1c.3; do
  printf 'PCI %s\n' "$device"
  for field in wakeup control runtime_status; do
    f=/sys/bus/pci/devices/$device/power/$field
    test ! -f "$f" || { printf '%s=' "$field"; cat "$f"; }
  done
  lspci -s "$device" -vv
done
for f in /sys/class/rtc/rtc*/name /sys/class/rtc/rtc*/device/power/wakeup /sys/class/rtc/rtc*/wakealarm /sys/power/wakeup_count /sys/power/pm_wakeup_irq; do
  test ! -f "$f" || { printf '%s=' "$f"; cat "$f"; }
done
cat /sys/kernel/debug/wakeup_sources
journalctl -b "${boot//-/}" --no-pager -o short-monotonic -u predator-pm-preserve-rtc.service -u predator-pm-clock-handoff.service -u systemd-timesyncd.service
