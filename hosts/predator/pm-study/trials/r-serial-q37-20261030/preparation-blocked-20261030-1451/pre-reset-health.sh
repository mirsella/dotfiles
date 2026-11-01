set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = 3c1873e4-b866-41e1-b411-702168ef90cf
cat /proc/sys/kernel/random/boot_id
cat /proc/uptime
expected=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
test "$(readlink -f /run/current-system)" = "$expected"
test "$(readlink -f /run/booted-system)" = "$expected"
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
predator-boot audit
systemctl list-jobs --no-pager
test -z "$(systemctl list-jobs --no-legend)"
systemctl --failed --no-pager
unexpected=$(systemctl list-units --failed --no-legend --plain | awk '$1 != "predator-pm-r-serial-q35r1-20261030.service"')
test -z "$unexpected"
systemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd
docker ps --format '{{.Names}} {{.Status}}'
test "$(zpool status -x)" = 'all pools are healthy'
zpool status -x
findmnt -rn -t zfs -o SOURCE,TARGET
test "$(timedatectl show -p NTPSynchronized --value)" = yes
timedatectl timesync-status
for f in /sys/power/suspend_stats/{success,fail} /sys/module/pm_stage/parameters/{armed,markers} /sys/power/{pm_async,pm_trace} /sys/class/power_supply/ACAD/online; do printf '%s=' "$f"; cat "$f"; done
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(cat /sys/power/pm_async)" = 1
test "$(cat /sys/module/pm_stage/parameters/markers)" = 0
test "$(cat /sys/module/pm_stage/parameters/armed)" = N
test ! -e /var/lib/predator-pm-study/HOLD
systemctl list-timers --all --no-pager 'predator-pm-*' 'night-suspend*' nextcloud-cron.timer
active=$(systemctl list-units --no-pager --no-legend --plain --state=active,activating,reloading,deactivating db-backup.service recovery-backup.service hermes-backup.service restic-backup.service sanoid.service 'syncoid-*.service' 'zfs-scrub*.service' 'zpool-trim*.service' nixos-upgrade.service nextcloud-setup.service nextcloud-cron.service)
printf 'active_maintenance=%s\n' "$active"
test -z "$active"
if pgrep -G nixbld; then echo 'Nix builders active'; exit 1; else test "$?" -eq 1; fi
sha256sum /nix/store/5gqrh1r71xnl127sj8dv83nnlk1nf6vs-predator-pm-study-0.1.0/bin/predator-pm-{study,control}
cd /home/mirsella/dev/dotfiles
sha256sum hosts/predator/pm-study/README.md hosts/predator/pm-study/trials/r-serial-q35r1-20261030/README.md pkgs/predator-pm-study/trial.rs
sudo -u mirsella git status --short
sudo -u mirsella git diff -- flake.lock
