set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H); test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=seconds
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend --plain)"
systemctl show systemd-suspend.service predator-pm-path-path-d3-v2-20261101.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
for p in 0000:03:00.1 0000:03:00.0 0000:00:1c.3; do
  for f in d3cold_allowed power_state power/wakeup power/control power/runtime_status; do printf '%s/%s=' "$p" "$f"; cat "/sys/bus/pci/devices/$p/$f"; done
  test "$(cat /sys/bus/pci/devices/$p/d3cold_allowed)" = 1
done
sites=$(grep -E '^drivers/pci/pci.c:2513 |^drivers/pci/pci-driver.c:(923|938) |^drivers/acpi/device_pm.c:(956|965|971) ' /sys/kernel/debug/dynamic_debug/control)
printf '%s\n' "$sites"
test "$(printf '%s\n' "$sites" | wc -l)" = 6
test "$(printf '%s\n' "$sites" | grep -c ' =_ ')" = 6
for d in study diagnostic instrumented s2idle wake-validation wake-path; do test ! -e "/var/lib/predator-pm-$d/HOLD"; done
ls -l /dev/tpm0 /dev/tpmrm0
test "$(readlink -f /sys/bus/acpi/devices/MSFT0101:00/driver)" = /sys/bus/acpi/drivers/tpm_crb
! grep -q crb_acpi_driver_init /proc/cmdline
! grep -Eq '^(acer_wmi|acer_wmi_ph31751) ' /proc/modules
test -z "$(journalctl -b -u predator-kbd-backlight-off.service --no-pager -o cat)"
printf 'TPM_CRB_Acer_rawEC_HOLD_and_restored_owned_settings=PASS\n'
for p in ff_rt_clk ff_pwr_btn ff_slp_btn gpe14 gpe66 sci sci_not; do printf '%s=' "$p"; cat "/sys/firmware/acpi/interrupts/$p"; done
cat /proc/interrupts
stat -c '%n %U %a %s' /var/lib/predator-pm-wake-path/path-d3-v2-20261101-*
sha256sum /var/lib/predator-pm-wake-path/path-d3-v2-20261101-*
journalctl -b -u systemd-logind --since '2026-11-01 09:13:00' --until '2026-11-01 09:40:00' --no-pager -o short-iso-precise
systemctl list-timers --all --no-pager 'predator-pm-path-*' night-suspend.timer predator-pm-next.timer
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock pkgs/predator-pm-study/{wake_path.rs,wake_validation.rs,wake_timing.rs,network.rs} hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/wake-path-20261101/{README.md,schedule.json} hosts/predator/pm-study/diagnostics/s2idle-wake-validation-20261031/README.md
date --iso-8601=seconds
