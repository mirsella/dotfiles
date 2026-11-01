set -euo pipefail
date --iso-8601=seconds
boot=$(cat /proc/sys/kernel/random/boot_id)
test "$boot" != 3c1873e4-b866-41e1-b411-702168ef90cf
printf 'BOOT=%s\n' "$boot"
grep '^btime ' /proc/stat
cat /proc/uptime
timeout 125s systemctl is-system-running --wait
date --iso-8601=seconds
expected=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(readlink -f /run/current-system)" = "$expected"
test "$(readlink -f /run/booted-system)" = "$expected"
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
predator-boot audit
systemctl --failed --no-pager
test -z "$(systemctl list-units --failed --no-legend --plain)"
systemctl list-jobs --no-pager
test -z "$(systemctl list-jobs --no-legend)"
for f in /sys/power/suspend_stats/{success,fail} /sys/module/pm_stage/parameters/{armed,markers} /sys/power/{pm_async,pm_trace,pm_debug_messages,pm_print_times,mem_sleep,pm_test} /sys/module/kernel/parameters/initcall_debug /sys/class/power_supply/ACAD/online /sys/class/graphics/fb0/blank; do printf '%s=' "$f"; cat "$f"; done
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/power/pm_async)" = 1
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test ! -e /var/lib/predator-pm-study/HOLD
ls -l /dev/tpm0 /dev/tpmrm0
readlink -f /sys/bus/acpi/devices/MSFT0101:00/driver
lsmod | grep -E 'acer|pm_stage|netconsole'
journalctl -b 0 -u predator-kbd-backlight-off.service --no-pager
cat /proc/acpi/button/lid/LID0/state
ethtool enp3s0f1
test "$(timedatectl show -p NTPSynchronized --value)" = yes
timedatectl timesync-status
nmcli -g GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
systemctl show predator-pm-clock-handoff.service -p FragmentPath -p ExecStart -p Result
zpool status
findmnt -rn -t zfs -o SOURCE,TARGET
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
docker ps --format '{{.Names}} {{.Status}}'
systemctl list-timers --all --no-pager 'predator-pm-*' 'night-suspend*' nextcloud-cron.timer
systemctl show nextcloud-cron.timer -p TimersMonotonic -p NextElapseUSecMonotonic -p LastTriggerUSecMonotonic -p AccuracyUSec -p RandomizedDelayUSec
active=$(systemctl list-units --no-pager --no-legend --plain --state=active,activating,reloading,deactivating db-backup.service recovery-backup.service hermes-backup.service restic-backup.service sanoid.service 'syncoid-*.service' 'zfs-scrub*.service' 'zpool-trim*.service' nixos-upgrade.service nextcloud-setup.service nextcloud-cron.service)
printf 'active_maintenance=%s\n' "$active"
test -z "$active"
if pgrep -G nixbld; then echo 'Nix builders active'; exit 1; else test "$?" -eq 1; fi
sha256sum /nix/store/5gqrh1r71xnl127sj8dv83nnlk1nf6vs-predator-pm-study-0.1.0/bin/predator-pm-{study,control}
for d in /sys/bus/usb/devices/*; do
  test -f "$d/idVendor" || continue
  printf 'USB %s\n' "$d"
  for f in busnum devpath idVendor idProduct manufacturer product serial speed power/control power/runtime_status; do
    if test -f "$d/$f"; then printf '%s=' "$f"; cat "$d/$f"; fi
  done
done
