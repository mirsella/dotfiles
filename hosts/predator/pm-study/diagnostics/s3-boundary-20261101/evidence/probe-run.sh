set -euo pipefail
export TZ=Europe/Paris
umask 077
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(systemctl show zfs-scrub.service -p ActiveState --value)" = inactive
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test -z "$(cat /sys/class/rtc/rtc0/wakealarm)"
test ! -d /sys/module/pm_boundary_probe
module=/nix/store/csz6rsq47hxsl16nbrrzqhadfgn41g9k-predator-pm-boundary-probe-20261101/lib/modules/6.18.58/extra/pm_boundary_probe.ko.xz
printf '9a10d09a4445282be5a8e026f7cfd298bfedb1cf68756b5ef9008dc3a40976cc  %s\n' "$module" | sha256sum -c -
dir=/var/lib/predator-pm-boundary/awake-20261101-a
test ! -e "$dir"
install -d -m 700 "$dir"
{
  date --iso-8601=ns
  cat /proc/sys/kernel/random/boot_id /proc/sys/kernel/tainted
  timedatectl show -p NTPSynchronized -p LocalRTC
  hwclock --show --utc
  cat /sys/power/mem_sleep /sys/power/pm_async /sys/power/pm_trace
  cat /sys/module/pm_stage/parameters/armed /sys/module/pm_stage/parameters/markers
  cat /sys/class/rtc/rtc0/wakealarm
} > "$dir/before.txt"
set +e
insmod "$module" nonce=awake-20261101-a boot_id=f9b45aa6-b820-4ab5-b18c-9324d8d53087 > "$dir/load.out" 2> "$dir/load.stderr"
load_status=$?
set -e
printf '%s\n' "$load_status" > "$dir/load.status"
if test "$load_status" -ne 0; then
  cat "$dir/load.stderr"
  journalctl -k -b --no-pager -o short-iso-precise | tail -35 > "$dir/failed-load-kernel.txt"
  cat "$dir/failed-load-kernel.txt"
  exit "$load_status"
fi
test "$(cat /sys/module/pm_boundary_probe/parameters/last_errno)" = -115
set +e
printf '1\n' > /sys/module/pm_boundary_probe/parameters/probe
probe_status=$?
set -e
printf '%s\n' "$probe_status" > "$dir/write.status"
{
  date --iso-8601=ns
  for field in nonce boot_id last_errno last_efi_status duration_ns; do
    printf '%s=' "$field"; cat "/sys/module/pm_boundary_probe/parameters/$field"
  done
} > "$dir/probe-result.txt"
set +e
cat /sys/module/pm_boundary_probe/parameters/readback > "$dir/readback.txt" 2> "$dir/readback.stderr"
read_status=$?
set -e
printf '%s\n' "$read_status" > "$dir/readback.status"
rmmod pm_boundary_probe
test ! -d /sys/module/pm_boundary_probe
{
  date --iso-8601=ns
  cat /proc/sys/kernel/random/boot_id /proc/sys/kernel/tainted
  timedatectl show -p NTPSynchronized -p LocalRTC
  hwclock --show --utc
  cat /sys/power/mem_sleep /sys/power/pm_async /sys/power/pm_trace
  cat /sys/module/pm_stage/parameters/armed /sys/module/pm_stage/parameters/markers
  cat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail
  cat /sys/class/rtc/rtc0/wakealarm
  systemctl show systemd-suspend.service -p ActiveState -p MainPID
  systemctl is-system-running
  systemctl list-jobs --no-legend
} > "$dir/after.txt"
journalctl -k -b --no-pager -o short-iso-precise | grep -E 'pm_boundary_probe|EFI.*(BUG|error)|Firmware Bug' > "$dir/kernel.txt"
cat "$dir/before.txt" "$dir/probe-result.txt" "$dir/readback.txt" "$dir/readback.stderr" "$dir/after.txt" "$dir/kernel.txt"
find "$dir" -maxdepth 1 -type f -printf '%u %m %s %f\n'
sha256sum "$dir"/*
test "$probe_status" = 0
test "$read_status" = 0
grep -Fx 'last_errno=0' "$dir/probe-result.txt"
printf 'awake_nonblocking_EFI_write_and_readback=PASS; reset_retention_and_suspended_context=UNPROVEN\n'
