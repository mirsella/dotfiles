set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
test "$(readlink -f /run/current-system)" = /nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(readlink -f /run/booted-system)" = "$(readlink -f /run/current-system)"
test "$(readlink -f /run/booted-system/kernel)" = /nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
test "$(readlink -f /run/booted-system/kernel-modules)" = /nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(systemctl is-system-running)" = running
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/power/pm_async)" = 1
test "$(cat /sys/power/pm_trace)" = 0
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test -z "$(cat /sys/class/rtc/rtc0/wakealarm)"
test "$(cat /sys/module/pstore/parameters/backend)" = efi_pstore
test "$(cat /sys/module/efi_pstore/parameters/pstore_disable)" = N
test "$(systemctl show zfs-scrub.service -p ActiveState --value)" = inactive
test ! -d /sys/module/pm_boundary_probe
test ! -e /nix/var/nix/gcroots/predator-pm-boundary-probe-20261101
date --iso-8601=ns
zpool status
timedatectl show -p NTPSynchronized -p LocalRTC
hwclock --show --utc
find /sys/firmware/efi/efivars -maxdepth 1 -name '*-47d95147-3665-4bda-8ddf-d48f0e339023' -printf '%f %s\n'
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study.nix pkgs/predator-pm-study/pm_stage.c hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md
test ! -e pkgs/predator-pm-study/boundary-probe.c
test ! -e pkgs/predator-pm-study/boundary-probe.nix
test ! -e .scratch/pm-s3-boundary-20261101
printf 'awake_probe_preflight=PASS\n'
