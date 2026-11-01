set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 1
test "$(cat /sys/power/suspend_stats/fail)" = 0
test ! -d /sys/module/pm_boundary_stage
test ! -e /var/lib/predator-pm-efi-s3-reviewed
test ! -e /nix/var/nix/gcroots/predator-pm-boundary-s3-reviewed-20261101
date --iso-8601=ns
cd /home/mirsella/dev/dotfiles
sudo -u mirsella git status --short
sudo -u mirsella git diff -- flake.lock
sha256sum flake.lock pkgs/predator-pm-study/Cargo.toml pkgs/predator-pm-study/boundary_trial.rs pkgs/predator-pm-study/boundary_record.rs hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/s3-efi-20261101/README.md
xz -dc | /etc/profiles/per-user/mirsella/bin/nix-store --import
/etc/profiles/per-user/mirsella/bin/nix-store --realise /nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0 --add-root /nix/var/nix/gcroots/predator-pm-boundary-s3-reviewed-20261101
echo "810564d7fc6d6ab3331220ce07e9c9a9959debde3d8299179c106881bef23e3b  /nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3" | sha256sum -c -
date --iso-8601=ns
