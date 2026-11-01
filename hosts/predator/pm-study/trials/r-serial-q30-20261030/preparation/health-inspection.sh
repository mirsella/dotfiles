set -euo pipefail
date --iso-8601=seconds
hostname
test "$(cat /proc/sys/kernel/random/boot_id)" = 8b7a173d-8406-4072-aebc-c85c8b4989e7
cat /proc/sys/kernel/random/boot_id
grep '^btime ' /proc/stat
cat /proc/uptime
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
uname -r
systemctl is-system-running
predator-boot audit
systemctl list-jobs --no-pager
systemctl --failed --no-pager
test ! -e /var/lib/predator-pm-study/HOLD
for f in /sys/power/suspend_stats/{success,fail} /sys/module/pm_stage/parameters/{armed,markers} /sys/power/{pm_async,pm_trace,pm_debug_messages,pm_print_times,mem_sleep,pm_test} /sys/module/kernel/parameters/initcall_debug /sys/class/power_supply/ACAD/online /sys/class/graphics/fb0/blank; do printf '%s=' "$f"; cat "$f"; done
ls -l /dev/tpm0 /dev/tpmrm0
readlink -f /sys/bus/acpi/devices/MSFT0101:00/driver
lsmod | grep -E 'acer|pm_stage|netconsole'
journalctl -b 0 -u predator-kbd-backlight-off.service --no-pager
cat /proc/acpi/button/lid/LID0/state
ethtool enp3s0f1
timedatectl show -p NTPSynchronized --value
timedatectl timesync-status
nmcli -g GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
systemctl show predator-pm-clock-handoff.service -p FragmentPath -p ExecStart -p Result
zpool status
findmnt -rn -t zfs -o SOURCE,TARGET
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud docker hermes-gateway hermes-dashboard systemd-timesyncd
docker ps --format '{{.Names}} {{.Status}}'
systemctl list-timers --all --no-pager 'predator-pm-*' 'night-suspend*'
systemctl list-units --no-pager --no-legend --plain --state=active,activating,reloading,deactivating db-backup.service recovery-backup.service hermes-backup.service restic-backup.service sanoid.service 'syncoid-*.service' 'zfs-scrub*.service' 'zpool-trim*.service' nixos-upgrade.service nextcloud-setup.service nextcloud-cron.service
if pgrep -G nixbld; then echo 'Nix builders active'; exit 1; else test "$?" -eq 1; fi
for d in /sys/bus/usb/devices/*; do
  test -f "$d/idVendor" || continue
  printf 'USB %s\n' "$d"
  for f in busnum devpath idVendor idProduct manufacturer product serial speed power/control power/runtime_status; do
    if test -f "$d/$f"; then printf '%s=' "$f"; cat "$d/$f"; fi
  done
done
