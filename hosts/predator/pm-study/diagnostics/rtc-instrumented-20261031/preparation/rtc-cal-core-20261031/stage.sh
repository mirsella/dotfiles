set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != "-" {print}')"
systemctl list-timers --all --no-pager nextcloud-cron.timer
systemctl show nextcloud-cron.service -p ActiveState -p ExecMainStartTimestamp -p ExecMainExitTimestamp
systemctl list-jobs --no-legend
install -d -m0700 /var/lib/predator-pm-instrumented
umask 077
set -o noclobber
cat > /var/lib/predator-pm-instrumented/rtc-cal-core-20261031-plan.conf <<'PLAN'
schema=rtc-instrumented-1
id=rtc-cal-core-20261031
boot=f8eb6e44-85ae-4e56-bc00-587876699945
runtime=/nix/store/bjpzwsx60fw6vf47yyakfqhgna8nkfqy-predator-pm-study-0.1.0/bin/predator-pm-instrumented
runtime_sha=a597160b303d59c49aaaf7e4a7716788adda32ad3a7fa9a62eb2bee8bfc3e56a
handoff_sha=1e7247c6e7e90a22fa41d022afb9e45001bf1328e69ec1440641f1c919518ee4
due=1793435700
wake=1793435700
window_end=1793484000
btime=1793381193
historical_btime=1793381192
success_before=9
markers_before=0
kind=calibration-core
predecessor=/var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt
predecessor_sha=7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc
pm_async=0
pm_trace=0
recorder=rtc-unsync-zerooffset
tpm=initialized
ec=not-run
measurement=dated-power-graph-required
reset_provenance=owner-recovery-mechanism-unspecified
nonce=rtc-cal-core-20261031-1793435700
PLAN
/nix/store/bjpzwsx60fw6vf47yyakfqhgna8nkfqy-predator-pm-study-0.1.0/bin/predator-pm-instrumented check /var/lib/predator-pm-instrumented/rtc-cal-core-20261031-plan.conf be16e14934c7e77e026fc7af61347c467fe4b54019a838a5b4132c2640d5833d
journalctl --sync
