set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
test "$(systemctl show systemd-suspend -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
systemctl show systemd-suspend.service predator-pm-boundary-s3-efi-v2-20261101.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
test ! -e /sys/module/pm_boundary_stage
test ! -e /sys/module/pm_boundary_compact
test ! -e /sys/module/pm_boundary_probe
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test -z "$(cat /sys/class/rtc/rtc0/wakealarm)"
test "$(cat /sys/power/pm_async)" = 1
test "$(cat /sys/power/pm_trace)" = 0
grep -F '[none]' /sys/power/pm_test
grep -F '[deep]' /sys/power/mem_sleep
for s in predator-pm-study predator-pm-diagnostic predator-pm-instrumented predator-pm-s2idle predator-pm-wake-validation predator-pm-wake-path predator-pm-boundary predator-pm-efi-s3 predator-pm-efi-s3-reviewed; do test ! -e /var/lib/$s/HOLD; done
test -c /dev/tpm0; test -c /dev/tpmrm0
readlink -e /sys/bus/acpi/devices/MSFT0101:00/driver
test "$(basename "$(readlink -e /sys/bus/acpi/devices/MSFT0101:00/driver)")" = tpm_crb
! grep -q crb_acpi_driver_init /proc/cmdline
! grep -E '^acer_wmi(_ph31751)? ' /proc/modules
test -z "$(journalctl -b 39d7579bb86d4d30b910131943e550c8 -u predator-kbd-backlight-off.service --no-pager -o cat)"
printf '%s\n' 'TPM_CRB_Acer_rawEC_HOLD_and_owned_cleanup=PASS'
grep '^btime ' /proc/stat
cat /proc/uptime
df -B1 /sys/firmware/efi/efivars
for p in ff_rt_clk ff_pwr_btn ff_slp_btn gpe14 gpe66 gpe69 gpe6D sci sci_not gpe_all error; do printf '%s=' "$p"; cat /sys/firmware/acpi/interrupts/$p; done
cat /proc/interrupts
for d in 0000:03:00.1 0000:03:00.0 0000:00:1c.3; do printf '%s\n' "$d"; for p in d3cold_allowed power_state power/wakeup power/control power/runtime_status; do printf '%s=' "$p"; cat /sys/bus/pci/devices/$d/$p; done; test "$(cat /sys/bus/pci/devices/$d/d3cold_allowed)" = 1; done
journalctl -b 39d7579bb86d4d30b910131943e550c8 -u systemd-logind --since '2026-11-01 14:18:00' --until '2026-11-01 14:30:00' --no-pager -o short-iso-precise
stat -c '%U %a %s %n' /var/lib/predator-pm-efi-s3-reviewed/*
sha256sum /var/lib/predator-pm-efi-s3-reviewed/*
sha256sum /var/lib/predator-pm-wake-validation/wake-20261031-plan.conf /var/lib/predator-pm-wake-validation/wake-20261031-completed.txt /var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf /var/lib/predator-pm-wake-path/path-d3-v2-20261101-completed.txt /var/lib/predator-pm-efi-s3/core-efi-20261101-{outcome,restored,stage-status,stage-readback}.txt
test ! -e /var/lib/predator-pm-efi-s3/core-efi-20261101-completed.json
sha256sum /nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3 /nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer nextcloud-cron.timer
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock pkgs/predator-pm-study/{boundary_s3.rs,boundary_trial.rs,boundary_record.rs,boundary-stage.c} hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/s3-efi-20261101/README.md
test ! -e hosts/predator/pm-study/diagnostics/s3-efi-20261101/review-1430
date --iso-8601=ns
