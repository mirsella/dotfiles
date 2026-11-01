set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-10-31
test "$(date +%s)" -lt 1793444700
test "$(cat /proc/sys/kernel/random/boot_id)" = ae29f675-9167-4975-a884-01885468bc38
cd /home/mirsella/dev/dotfiles
sync=.scratch/pm-s2idle-sync-20261031
report=hosts/predator/pm-study/diagnostics/s2idle-control-20261031
date --iso-8601=seconds
git status --short
git diff -- flake.lock
sha256sum -c "$sync/preimages.sha256"
sha256sum -c "$sync/payload.sha256"
mkdir "$sync/incoming" "$sync/before"
tar -xzf "$sync/payload.tar.gz" -C "$sync/incoming"
new=("$report" pkgs/predator-pm-study/s2idle.rs)
existing=(hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md pkgs/predator-pm-study/Cargo.toml)
for p in "${new[@]}"; do test ! -e "$p"; done
for p in "${existing[@]}"; do test ! -e "$p.s2idle-new"; done
(cd "$sync/incoming/$report/preparation" && sha256sum -c SHA256SUMS >/dev/null)
sha256sum -c "$sync/preimages.sha256"
for p in "${new[@]}"; do cp -a "$sync/incoming/$p" "$p"; chown -R --reference=hosts/predator/pm-study/README.md "$p"; done
for p in "${existing[@]}"; do
    mkdir -p "$sync/before/$(dirname "$p")"
    cp -a "$p" "$sync/before/$p"
    cp "$sync/incoming/$p" "$p.s2idle-new"
    chmod --reference="$p" "$p.s2idle-new"
    chown --reference="$p" "$p.s2idle-new"
    mv "$p.s2idle-new" "$p"
done
sha256sum -c "$sync/final.sha256"
(cd "$report/preparation" && sha256sum -c SHA256SUMS >/dev/null)
git diff --check -- hosts/predator/pm-study pkgs/predator-pm-study/Cargo.toml pkgs/predator-pm-study/s2idle.rs
date --iso-8601=seconds
printf 'guarded_s2idle_preparation_and_source_sync=PASS\n'
