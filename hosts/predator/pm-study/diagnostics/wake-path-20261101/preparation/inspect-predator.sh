set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
date -Is
cat /proc/sys/kernel/random/boot_id
grep '^btime ' /proc/stat
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
test "$(awk '$1=="btime" {print $2}' /proc/stat)" = 1793474520
systemctl is-system-running
systemctl show systemd-suspend.service -p ActiveState -p MainPID -p Result
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
systemctl list-jobs --no-pager
test -z "$(systemctl list-jobs --no-legend --plain)"
for f in /sys/power/suspend_stats/success /sys/power/suspend_stats/fail /sys/power/mem_sleep /sys/power/pm_test /sys/power/pm_async /sys/power/pm_trace /sys/module/pm_stage/parameters/* /sys/class/rtc/rtc0/wakealarm; do printf '%s=' "$f"; cat "$f"; done
test "$(cat /sys/power/suspend_stats/success)" = 1
test "$(cat /sys/power/suspend_stats/fail)" = 0
cat /sys/kernel/debug/wakeup_sources
cat /proc/acpi/wakeup
cat /proc/interrupts
for f in /sys/firmware/acpi/interrupts/*; do printf '%s=' "$f"; cat "$f"; done
ip -details address show
ip route show
nmcli -f GENERAL,IP4 device show enp3s0f1
ethtool enp3s0f1
ethtool -i enp3s0f1
for n in /sys/class/net/*; do
  test -e "$n/device" || continue
  printf 'NETWORK_DEVICE %s\n' "$n"
  cat "$n/address"
  p=$(readlink -e "$n/device")
  while test "$p" != /sys/devices && test "$p" != /sys; do
    printf 'ANCESTOR %s\n' "$p"
    for f in vendor device class power_state d3cold_allowed power/wakeup power/control power/runtime_status power/runtime_active_time power/runtime_suspended_time; do
      if test -e "$p/$f"; then printf '%s=' "$f"; cat "$p/$f"; else printf '%s=ABSENT\n' "$f"; fi
    done
    for f in driver firmware_node; do if test -L "$p/$f"; then readlink -e "$p/$f"; fi; done
    if test -e "$p/config"; then od -Ax -tx1 -N 256 "$p/config"; fi
    p=$(dirname "$p")
  done
done
for z in /sys/class/thermal/thermal_zone*; do printf '%s ' "$z"; cat "$z/type" "$z/temp"; done
for c in tcpdump lspci setpci; do if command -v "$c"; then "$c" --version; else printf 'COMMAND_UNAVAILABLE=%s\n' "$c"; fi; done
timedatectl
timedatectl timesync-status
hwclock --show --utc
cat /proc/driver/rtc
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer nextcloud-cron.timer
systemctl --failed --no-pager
sha256sum /var/lib/predator-pm-wake-validation/wake-20261031-plan.conf /var/lib/predator-pm-wake-validation/wake-20261031-completed.txt
cat /var/lib/predator-pm-wake-validation/wake-20261031-completed.txt
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/s2idle-wake-validation-20261031/README.md
date -Is
printf 'read_only_wake_path_inspection=PASS\n'
