set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = ae29f675-9167-4975-a884-01885468bc38
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != "-" {print}')"
systemctl list-timers --all --no-pager nextcloud-cron.timer
systemctl show nextcloud-cron.service -p ActiveState -p ExecMainStartTimestamp -p ExecMainExitTimestamp
systemctl list-jobs --no-legend
install -d -m0700 /var/lib/predator-pm-s2idle
umask 077
set -o noclobber
cat > /var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf <<'PLAN'
schema=rtc-s2idle-control-1
id=s2idle-20261031
boot=ae29f675-9167-4975-a884-01885468bc38
runtime=/nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0/bin/predator-pm-s2idle
runtime_sha=27c4152e2577d3569027fa479a412cb08e63f41a63d99755b45a6e17eaad5a33
source_sha=618ecaee583e6b2a2a1489649418731ddbfcad3c65f2e9d109772f4c6db602df
handoff_sha=1e7247c6e7e90a22fa41d022afb9e45001bf1328e69ec1440641f1c919518ee4
due=1793444820
wake=1793445780
window_end=1793484000
btime=1793440484
historical_btime=1793440484
success_before=0
markers_before=0
kind=s2idle-control
recovery_sha=b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55
pm_async=0
pm_trace=0
pm_test=none
mem_sleep=s2idle
recorder=rtc-unsync-zerooffset
tpm=initialized
ec=not-run
measurement=dated-power-graph-required
reset_provenance=owner-forced-reboot-method-off-duration-unspecified
nonce=s2idle-20261031-1793444820
PLAN
/nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0/bin/predator-pm-s2idle check /var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820
journalctl --sync
