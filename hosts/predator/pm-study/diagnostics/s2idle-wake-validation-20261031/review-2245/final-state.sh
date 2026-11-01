set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-10-31
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=seconds
systemctl show systemd-suspend.service predator-pm-wake-wake-20261031.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-jobs --no-pager
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer
test -e /dev/tpm0 && test -e /dev/tpmrm0
readlink -e /sys/bus/acpi/devices/MSFT0101:00/driver
test "$(readlink -e /sys/bus/acpi/devices/MSFT0101:00/driver)" = /sys/bus/acpi/drivers/tpm_crb
! grep -q crb_acpi_driver_init /proc/cmdline
test ! -e /sys/module/acer_wmi && test ! -e /sys/module/acer_wmi_ph31751
journalctl -b -u predator-kbd-backlight-off.service --no-pager -o cat
for dir in predator-pm-study predator-pm-diagnostic predator-pm-instrumented predator-pm-s2idle predator-pm-wake-validation; do test ! -e "/var/lib/$dir/HOLD"; done
printf 'TPM_CRB_and_Acer_exclusions_and_all_HOLD_checks=PASS\n'
cat /proc/interrupts | grep -E '^[[:space:]]*(8|9):'
for p in /sys/firmware/acpi/interrupts/ff_rt_clk /sys/firmware/acpi/interrupts/ff_pwr_btn /sys/firmware/acpi/interrupts/ff_slp_btn; do printf '%s=' "$p"; cat "$p"; done
sha256sum /var/lib/predator-pm-study/f9b45aa6-b820-4ab5-b18c-9324d8d53087-boot-rtc.txt /var/lib/predator-pm-study/last-completed.txt /var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt /var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-completed.txt /var/lib/predator-pm-instrumented/rtc-s3-20261031-plan.conf /var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf /var/lib/predator-pm-s2idle/s2idle-20261031-raw-return.txt
sha256sum /nix/store/y5x8nw8lvhjjmxp9m1aiy4hkgsfsrr7g-predator-pm-study-0.1.0/bin/predator-pm-wake-validation /nix/store/qzs6qmgvvka19nv4dyihz9rc8mbybkkv-predator-pm-study-0.1.0/bin/predator-pm-study
stat -c '%n %U %a' /var/lib/predator-pm-wake-validation/wake-20261031-*
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/s2idle-wake-validation-20261031/README.md pkgs/predator-pm-study/wake_validation.rs pkgs/predator-pm-study/wake_timing.rs pkgs/predator-pm-study/network.rs
test ! -e hosts/predator/pm-study/diagnostics/s2idle-wake-validation-20261031/review-2245
date --iso-8601=seconds
printf 'final_read_only_state_capture=PASS\n'
