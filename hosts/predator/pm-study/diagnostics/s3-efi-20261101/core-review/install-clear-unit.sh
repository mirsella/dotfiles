set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
echo "58db33f0bc4372f3f928835c4db1433212c18379be51420a90bd6e57f5ceb065  flake.lock" | sha256sum -c -
d=.scratch/pm-core-unit-state-20261101
test ! -e "$d"
mkdir -m700 "$d"
tar -xJf - -C "$d"
cd "$d"
echo "209796623be0ebd93566b46bd7cf676953ed97859fd7ff98f6acfed219ab6a04  clear-failed-unit" | sha256sum -c -
chmod 700 clear-failed-unit
./clear-failed-unit
