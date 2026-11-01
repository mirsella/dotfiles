set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
cat /proc/sys/kernel/random/boot_id /proc/uptime /sys/devices/system/cpu/online
for p in /run/current-system /run/booted-system /run/current-system/kernel /run/current-system/kernel-modules; do readlink -f "$p"; done
for p in /sys/power/suspend_stats/success /sys/power/suspend_stats/fail /sys/power/pm_test /sys/power/pm_async /sys/power/pm_trace /sys/module/pm_stage/parameters/armed /sys/module/pm_stage/parameters/markers; do printf '%s=' "$p"; cat "$p"; done
test "$(cat /sys/power/suspend_stats/success)" = 9
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/power/pm_async)" = 1
grep -q '\[none\]' /sys/power/pm_test
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend --plain)"
test -z "$(systemctl list-units --failed --no-legend --plain)"
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != "-" {print}')"
test ! -e /var/lib/predator-pm-diagnostic/d-core-01-20261030-requested.txt
test ! -e /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-completed.txt
sha256sum /var/lib/predator-pm-study/last-completed.txt /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-revalidated.txt /var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt
nmcli -g GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS device show enp3s0f1
timedatectl show -p NTPSynchronized
zpool status -x
for p in /srv/data/fast /var/lib/nextcloud/data /srv/backup /srv/backup/recovery /srv/data/archive; do findmnt -n -o SOURCE,TARGET --mountpoint "$p"; done
podman ps --format '{{.Names}} {{.Status}}'
cd /home/mirsella/dev/dotfiles
sha256sum pkgs/predator-pm-study/diagnostic.rs
for root in .scratch/pm-resume-20261028/deployed-system .scratch/pm-serial-q35r1-20261030/runtime-root .scratch/pm-phase-diagnostics-20261030/runtime-root .scratch/pm-diagnostic-readiness-20261030/runtime-root .scratch/pm-core-timekeeping-check-20261030/runtime-root; do printf '%s=' "$root"; readlink -f "$root"; done
