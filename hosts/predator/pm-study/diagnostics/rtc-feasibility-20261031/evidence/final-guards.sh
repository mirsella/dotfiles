set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-10-31
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
test "$(cat /sys/power/suspend_stats/success)" = 9
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/power/pm_async)" = 1
test "$(cat /sys/power/pm_trace)" = 0
grep -q '\[none\]' /sys/power/pm_test
grep -q '\[deep\]' /sys/power/mem_sleep
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test -z "$(cat /sys/class/rtc/rtc0/wakealarm)"
test -z "$(systemctl list-jobs --no-legend)"
test -z "$(systemctl --failed --no-legend)"
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != "-" {print}')"
test "$(readlink -e /run/current-system)" = /nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(readlink -e /run/booted-system)" = /nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(readlink -e /run/current-system/kernel)" = /nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
test "$(readlink -e /run/current-system/kernel-modules)" = /nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
test "$(sha256sum /var/lib/predator-pm-study/last-completed.txt | cut -d' ' -f1)" = 280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08
test "$(sha256sum /var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt | cut -d' ' -f1)" = 7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc
test "$(timedatectl show -p NTPSynchronized --value)" = yes
systemctl show systemd-suspend.service -p ActiveState -p MainPID
cat /proc/sys/kernel/random/boot_id
grep '^btime ' /proc/stat
cat /proc/uptime
cat /sys/devices/system/cpu/online
test ! -e /var/lib/predator-pm-study/HOLD
test ! -e /var/lib/predator-pm-diagnostic/HOLD
printf 'final_guard_result=PASS_NO_NEW_PM_OPERATION\n'
