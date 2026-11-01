set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
boot=$(cat /proc/sys/kernel/random/boot_id)
test "$boot" != f9b45aa6-b820-4ab5-b18c-9324d8d53087
btime=$(awk '/^btime / {print $2}' /proc/stat)
test "$btime" -ge 1793530730
test "$btime" -le 1793531030
date --iso-8601=ns
printf 'new_boot=%s\nbtime=%s\n' "$boot" "$btime"
for p in /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules; do readlink -f "$p"; done
test "$(readlink -f /run/booted-system)" = /nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(readlink -f /run/current-system)" = "$(readlink -f /run/booted-system)"
test "$(readlink -f /run/booted-system/kernel)" = /nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
test "$(readlink -f /run/booted-system/kernel-modules)" = /nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
df -B1 /sys/firmware/efi/efivars
stat -f /sys/firmware/efi/efivars
find /sys/firmware/efi/efivars -maxdepth 1 -name '*-47d95147-3665-4bda-8ddf-d48f0e339023' -printf '%f %s\n'
test ! -d /sys/module/pm_boundary_probe
systemctl list-jobs --no-pager
systemctl --failed --no-pager
systemctl show systemd-suspend.service predator-pm-clock-handoff.service predator-pm-preserve-rtc.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer
cat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail
for p in /sys/power/pm_async /sys/power/pm_trace /sys/power/mem_sleep /sys/power/pm_test /sys/class/rtc/rtc0/wakealarm; do printf '%s=' "$p"; cat "$p"; done
timedatectl status
nmcli -g GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
journalctl -b -u predator-pm-clock-handoff.service -u systemd-timesyncd.service --no-pager -o short-iso-precise
cat /var/lib/predator-pm-boundary/normal-reboot-20261101/requested.txt
