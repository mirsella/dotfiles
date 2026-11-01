set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
date --iso-8601=ns
lsblk -b -o NAME,TYPE,SIZE,TRAN,MODEL,SERIAL,MOUNTPOINTS
zpool status -P
for d in /sys/class/block/sd?; do
  printf '\nBLOCK %s %s\n' "$d" "$(readlink -e "$d/device")"
  for f in device/vendor device/model device/state device/power/runtime_status device/power/control queue/rotational; do
    if test -r "$d/$f"; then printf '%s=' "$f"; cat "$d/$f"; fi
  done
done
for d in /sys/bus/usb/devices/*; do
  test -f "$d/idVendor" || continue
  printf '\nUSB %s %s\n' "$d" "$(readlink -e "$d")"
  for f in idVendor idProduct manufacturer product serial bMaxPower bmAttributes speed power/control power/runtime_status power/wakeup; do
    if test -r "$d/$f"; then printf '%s=' "$f"; cat "$d/$f"; fi
  done
done
for d in /sys/class/power_supply/*; do
  printf '\nPOWER %s\n' "$d"
  cat "$d/uevent"
done
for d in /sys/bus/pci/devices/*; do
  printf '\nPCI %s driver=%s\n' "$d" "$(readlink -e "$d/driver" || true)"
  for f in class vendor device power_state power/control power/runtime_status power/wakeup d3cold_allowed; do
    if test -r "$d/$f"; then printf '%s=' "$f"; cat "$d/$f"; fi
  done
done
df -B1 /sys/firmware/efi/efivars /boot
systemctl show systemd-suspend.service -p ActiveState -p MainPID -p Result
systemctl list-timers --all --no-pager 'predator-pm-*' nextcloud-cron.timer night-suspend.timer
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock pkgs/predator-pm-study/Cargo.toml pkgs/predator-pm-study/boundary_s3.rs pkgs/predator-pm-study/boundary_trial.rs pkgs/predator-pm-study/boundary_record.rs hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/s3-efi-20261101/README.md
date --iso-8601=ns
