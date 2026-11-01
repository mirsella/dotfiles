set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = bf51c3ac-164e-4bfa-b56c-7b71df526b7f
dir=/home/mirsella/dev/dotfiles/.scratch/pm-efi-compact-20261101
test ! -e "$dir"
test ! -e /nix/var/nix/gcroots/predator-pm-boundary-compact-20261101
mkdir -m 700 "$dir"
tar -xJf - -C "$dir"
cd "$dir"
sha256sum --check transfer.sha256
xz -dc module-export.nar.xz | /etc/profiles/per-user/mirsella/bin/nix-store --import
/etc/profiles/per-user/mirsella/bin/nix-store --add-root /nix/var/nix/gcroots/predator-pm-boundary-compact-20261101 --realise /nix/store/3dzvzp3hg82n7gkrmrbac2svplh4xlzb-predator-pm-boundary-compact-20261101
sha256sum /nix/store/3dzvzp3hg82n7gkrmrbac2svplh4xlzb-predator-pm-boundary-compact-20261101/lib/modules/6.18.58/extra/pm_boundary_compact.ko.xz
chmod 700 operate
date --iso-8601=seconds
