set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = bf51c3ac-164e-4bfa-b56c-7b71df526b7f
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(systemctl is-system-running)" = running
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test ! -d /sys/module/pm_boundary_probe
test ! -d /sys/module/pm_boundary_compact
test -z "$(cat /sys/class/rtc/rtc0/wakealarm)"
date --iso-8601=ns
cat /proc/sys/kernel/random/boot_id
grep '^btime ' /proc/stat
cat /proc/sys/kernel/tainted
df -B1 /sys/firmware/efi/efivars
stat -f /sys/firmware/efi/efivars
printf 'efivarfs visible private names (direct firmware creation may not populate the mounted filesystem):\n'
find /sys/firmware/efi/efivars -maxdepth 1 -name '*-47d95147-3665-4bda-8ddf-d48f0e339023' -printf '%f %s\n'
test -c /dev/tpm0; test -c /dev/tpmrm0
test "$(readlink -f /sys/bus/acpi/devices/MSFT0101:00/driver)" = /sys/bus/acpi/drivers/tpm_crb
! grep -q crb_acpi_driver_init /proc/cmdline
! grep -Eq '^acer_wmi(_ph31751)? ' /proc/modules
test -z "$(journalctl -b -u predator-kbd-backlight-off.service -o cat --no-pager)"
for dir in predator-pm-study predator-pm-diagnostic predator-pm-instrumented predator-pm-s2idle predator-pm-wake-validation predator-pm-wake-path predator-pm-boundary; do test ! -e "/var/lib/$dir/HOLD"; done
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer
systemctl show systemd-suspend.service predator-pm-clock-handoff.service predator-pm-preserve-rtc.service zfs-scrub.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus
sha256sum /var/lib/predator-pm-wake-validation/wake-20261031-plan.conf /var/lib/predator-pm-wake-validation/wake-20261031-completed.txt /var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf /var/lib/predator-pm-wake-path/path-d3-v2-20261101-completed.txt /var/lib/predator-pm-study/f9b45aa6-b820-4ab5-b18c-9324d8d53087-boot-rtc.txt /var/lib/predator-pm-study/bf51c3ac-164e-4bfa-b56c-7b71df526b7f-boot-rtc.txt /var/lib/predator-pm-boundary/normal-reboot-20261101/requested.txt
find /var/lib/predator-pm-boundary -maxdepth 2 -type f -printf '%u %m %s %p\n'
find /nix/var/nix/gcroots -maxdepth 1 -name '*predator-pm*' -printf '%f -> %l\n'
sha256sum /var/lib/predator-pm-boundary/compact-20261101-a/*
printf 'awake_probe_cleanup_security_history_and_normal_state=PASS\n'
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study.nix pkgs/predator-pm-study/pm_stage.c pkgs/predator-pm-study/wake_path.rs pkgs/predator-pm-study/wake_validation.rs pkgs/predator-pm-study/wake_timing.rs hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md
test ! -e hosts/predator/pm-study/diagnostics/efi-compact-20261101
test ! -e pkgs/predator-pm-study/boundary-compact.c
test ! -e pkgs/predator-pm-study/boundary-compact.nix
