set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
test -z "$(systemctl list-jobs --no-legend)"
test -z "$(systemctl list-timers --all --no-legend | grep -E 'predator-pm-(boundary|path|wake|diagnostic|instrumented|s2idle)-' | grep -v '^-' || true)"
install -d -m700 /var/lib/predator-pm-efi-s3
umask 077
set -o noclobber
cat > /var/lib/predator-pm-efi-s3/core-efi-20261101-plan.conf <<'PLAN'
schema=efi-s3-20261101-1
id=core-efi-20261101
nonce=core1101a
boot=39d7579b-b86d-4d30-b910-131943e550c8
btime=1793535078
mode=core
due=1793537100
rtc=0
review=1793538000
runtime=/nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial
runtime_sha=12b551f868bb0e0eae8c012bc9e41a0fbbc6e4dc975bf651e3a538e1651ed886
source_sha=4884bb9d16b073785e441a17f807a6ce130f53e6b43a0a0cbdb9e41cd0a19f7f
module=/nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz
module_sha=90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0
success_before=0
core_completion_sha=none
retention_sha=176cbbc62c46cd68c977a07d6c3f775d770ff0983176190646c17f9c18d37b63
owner_ready=owner-confirmed-ready-20261101
pm_async=0
pm_trace=0
pm_test=core
PLAN
/nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial check /var/lib/predator-pm-efi-s3/core-efi-20261101-plan.conf 95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3
journalctl --sync
