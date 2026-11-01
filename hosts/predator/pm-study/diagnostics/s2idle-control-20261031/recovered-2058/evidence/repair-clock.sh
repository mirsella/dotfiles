set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
test "$(readlink -f /run/current-system)" = /nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test "$(cat /sys/power/pm_test)" = '[none] core processors platform devices freezer'
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(nmcli -g GENERAL.CON-UUID device show enp3s0f1)" = c4e6863b-7f76-4a4d-9a48-9937af0b5a36
test "$(nmcli -g GENERAL.STATE device show enp3s0f1)" = '100 (connected)'
systemctl show predator-pm-clock-handoff.service -p ActiveState -p Result -p ExecMainStatus
systemctl restart predator-pm-clock-handoff.service
systemctl show predator-pm-clock-handoff.service -p ActiveState -p Result -p ExecMainStatus
test "$(systemctl show predator-pm-clock-handoff.service -p ActiveState --value)" = active
test "$(systemctl show predator-pm-clock-handoff.service -p Result --value)" = success
test "$(timedatectl show -p NTPSynchronized --value)" = yes
systemctl --failed --no-pager
timedatectl timesync-status
hwclock --show --utc
cat /sys/power/suspend_stats/{success,fail}
