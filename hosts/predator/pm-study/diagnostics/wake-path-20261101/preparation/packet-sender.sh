set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
date --iso-8601=ns
test "$(cat /proc/sys/kernel/random/boot_id)" = 2142644e-84db-40ae-b1b9-9e8c71265cf7
test "$(timedatectl show -p NTPSynchronized --value)" = yes
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
printf 'AWAKE_ONLY_PACKET_PROOF label=awake-packet-20261101-0835 sends=1 payload=canonical102bytes\n'
ip route get 192.168.1.255
date --iso-8601=ns
/usr/bin/python3 /home/mirsella/wol_predator.py
date --iso-8601=ns
