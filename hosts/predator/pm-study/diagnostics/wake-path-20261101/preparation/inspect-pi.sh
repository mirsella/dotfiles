set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
date -Is
cat /proc/sys/kernel/random/boot_id
timedatectl
timedatectl timesync-status
systemctl show predator-netconsole-20261006.service -p ActiveState -p MainPID
systemctl list-jobs --no-pager
systemctl list-timers --all --no-pager 'predator-pm-*'
ip -details address show
ip route get 192.168.1.255
sha256sum /home/mirsella/wol_predator.py
cat /home/mirsella/wol_predator.py
if command -v tcpdump; then tcpdump --version; else printf 'COMMAND_UNAVAILABLE=tcpdump\n'; fi
date -Is
printf 'read_only_observer_inspection=PASS\n'
