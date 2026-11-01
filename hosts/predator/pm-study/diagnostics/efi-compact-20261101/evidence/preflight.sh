set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = bf51c3ac-164e-4bfa-b56c-7b71df526b7f
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
test ! -d /sys/module/pm_boundary_probe
test ! -d /sys/module/pm_boundary_compact
test ! -e /var/lib/predator-pm-boundary/compact-20261101-a
test ! -e /nix/var/nix/gcroots/predator-pm-boundary-compact-20261101
test -z "$(find /sys/firmware/efi/efivars -maxdepth 1 -name 'PMS3-20261101-a-*' -print -quit)"
date --iso-8601=ns
df -B1 /boot /sys/firmware/efi/efivars
systemctl show zfs-scrub.service nextcloud-cron.service -p Id -p ActiveState -p Result
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study.nix pkgs/predator-pm-study/boundary-probe.c pkgs/predator-pm-study/boundary-probe.nix hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md
test ! -e .scratch/pm-efi-compact-20261101
test ! -e pkgs/predator-pm-study/boundary-compact.c
test ! -e pkgs/predator-pm-study/boundary-compact.nix
printf 'compact_probe_preflight=PASS\n'
