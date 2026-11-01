set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
test "$(date +%s)" -ge 1793516400
test "$(date +%s)" -lt 1793545800
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
test ! -e /sys/module/pm_boundary_stage
test ! -e /var/lib/predator-pm-recorder-off
test ! -e /nix/var/nix/gcroots/predator-pm-recorder-off-20261101
date --iso-8601=ns
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock pkgs/predator-pm-study/Cargo.toml pkgs/predator-pm-study/boundary_s3.rs pkgs/predator-pm-study/boundary_trial.rs pkgs/predator-pm-study/boundary_record.rs hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md hosts/predator/pm-study/diagnostics/s3-efi-20261101/README.md
xz -dc | /etc/profiles/per-user/mirsella/bin/nix-store --import
/etc/profiles/per-user/mirsella/bin/nix-store --realise /nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0 --add-root /nix/var/nix/gcroots/predator-pm-recorder-off-20261101
echo "94b44bc3d6c5a4666372cee0cb47176966b2d684060bd26145fe856975d5d377  /nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0/bin/predator-pm-recorder-off" | sha256sum -c -
date --iso-8601=ns
