set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
date --iso-8601=ns
cd /home/mirsella/dev/dotfiles
sudo -u mirsella git status --short
sudo -u mirsella git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study.nix pkgs/predator-pm-study/Cargo.toml pkgs/predator-pm-study/wake_path.rs hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md
test ! -e /nix/var/nix/gcroots/predator-pm-boundary-stage-20261101
test ! -e /nix/var/nix/gcroots/predator-pm-boundary-trial-20261101
xz -dc | /etc/profiles/per-user/mirsella/bin/nix-store --import
/etc/profiles/per-user/mirsella/bin/nix-store --realise /nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101 --add-root /nix/var/nix/gcroots/predator-pm-boundary-stage-20261101
/etc/profiles/per-user/mirsella/bin/nix-store --realise /nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0 --add-root /nix/var/nix/gcroots/predator-pm-boundary-trial-20261101
sha256sum /nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz /nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial
date --iso-8601=ns
