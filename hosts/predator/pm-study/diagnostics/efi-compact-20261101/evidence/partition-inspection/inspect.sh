set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = bf51c3ac-164e-4bfa-b56c-7b71df526b7f
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
date --iso-8601=seconds
findmnt /boot
df -B1 /boot /sys/firmware/efi/efivars
du -B1 -s /boot/EFI/Linux /boot/EFI/systemd /boot/EFI/BOOT
find /boot/EFI/Linux -maxdepth 1 -type f -name '*.efi' -printf '%s %f\n' | sort -n
bootctl list --json=short
stat -f /sys/firmware/efi/efivars
for pattern in 'Boot[0-9A-F][0-9A-F][0-9A-F][0-9A-F]-*' 'dump-type*' '*-47d95147-3665-4bda-8ddf-d48f0e339023'; do
  printf 'variable-name pattern=%s\n' "$pattern"
  find /sys/firmware/efi/efivars -maxdepth 1 -type f -name "$pattern" -printf '%s %f\n'
done
cat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail
