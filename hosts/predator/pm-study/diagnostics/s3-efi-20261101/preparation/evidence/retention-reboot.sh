set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
old=bf51c3ac-164e-4bfa-b56c-7b71df526b7f
system=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
entry=nixos-generation-167-o7mzz3pnl4dj6n4ajwtxp6rhpuc6k4pbdzfggpkl3govxscp665q.efi
test "$(cat /proc/sys/kernel/random/boot_id)" = "$old"
test "$(readlink -f /run/current-system)" = "$system"
test "$(readlink -f /run/booted-system)" = "$system"
test "$(readlink -f /run/booted-system/kernel)" = /nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
test "$(readlink -f /run/booted-system/kernel-modules)" = /nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
bootctl list --json=short | jq -e --arg id "$entry" '[.[] | select(.isDefault == true)] | length == 1 and .[0].id == $id and .[0].isSelected == true' >/dev/null
printf '63a2619ff831156ba51c08c537efafa47a5c4c1e07ff5c52a7253435c7acdcd3  /boot/EFI/Linux/%s\n' "$entry" | sha256sum -c -
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(systemctl is-system-running)" = running
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/power/pm_trace)" = 0
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test -z "$(cat /sys/class/rtc/rtc0/wakealarm)"
test ! -d /sys/module/pm_boundary_probe
test ! -d /sys/module/pm_boundary_compact
test -z "$(systemctl list-units --no-legend --state=active,activating,reloading,deactivating db-backup.service recovery-backup.service hermes-backup.service restic-backup.service sanoid.service 'syncoid-*.service' 'zfs-scrub*.service' 'zpool-trim*.service' nixos-upgrade.service nextcloud-setup.service nextcloud-cron.service)"
if pgrep -a -G nixbld; then exit 1; else test "$?" = 1; fi
dir=/var/lib/predator-pm-boundary/retention-reboot-20261101
test ! -e "$dir"
umask 077
mkdir -m 700 "$dir"
{
  date --iso-8601=ns
  printf 'authorization=owner requested suspend-hang tests and confirmed Ready now; announced recorder retention reboot\nold_boot=%s\nexpected_system=%s\nexpected_entry=%s\n' "$old" "$system" "$entry"
  printf 'requested_epoch=%s\nold_success=0\nold_fail=0\nnew_EFI_writes=0\nS3_attempts=0\nretention_variable=PMS3-20261101-a-47d95147-3665-4bda-8ddf-d48f0e339023\nexpected_record_crc=5c258a74\n' "$(date +%s)"
  df -B1 /sys/firmware/efi/efivars
} > "$dir/requested.txt"
journalctl --sync
sync
cat "$dir/requested.txt"
printf 'issuing exactly one ordinary systemctl reboot\n'
systemctl reboot
