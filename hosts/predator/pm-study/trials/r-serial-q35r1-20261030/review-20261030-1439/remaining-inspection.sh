set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = 3c1873e4-b866-41e1-b411-702168ef90cf
boot=3c1873e4b86641e1b411702168ef90cf
journalctl -b "$boot" -u nextcloud-cron.service -u nextcloud-cron.timer --since '2026-10-30 14:10:00' --until '2026-10-30 14:15:00' --no-pager -o short-iso-precise
journalctl -b "$boot" -u systemd-suspend.service --no-pager -o short-iso-precise
dir=/var/lib/predator-pm-study
for file in "$dir"/r-serial-q35r1-20261030-* "$dir/last-completed.txt"; do
  stat -c '%n %s %y' "$file"
  sha256sum "$file"
done
for suffix in return after-kernel after-service verification-kernel verification kernel-completed completed; do
  file="$dir/r-serial-q35r1-20261030-$suffix.txt"
  if test -e "$file"; then printf 'PRESENT %s\n' "$file"; else printf 'ABSENT %s\n' "$file"; fi
done
cat "$dir/last-completed.txt"
timedatectl show -p NTPSynchronized --value
timedatectl timesync-status
journalctl -b "$boot" -u systemd-timesyncd --no-pager -o short-iso-precise
zpool status
findmnt -rn -t zfs -o SOURCE,TARGET
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
docker ps --format '{{.Names}} {{.Status}}'
ps -eo pid,ppid,etimes,args
cd /home/mirsella/dev/dotfiles
sudo -u mirsella git status --short
sudo -u mirsella git diff -- flake.lock
sha256sum hosts/predator/pm-study/README.md hosts/predator/pm-study/trials/r-serial-q35r1-20261030/README.md pkgs/predator-pm-study/trial.rs
