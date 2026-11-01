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
cat > /var/lib/predator-pm-instrumented/rtc-s3-20261031-plan.conf <<'PLAN'
schema=rtc-instrumented-1
id=rtc-s3-20261031
boot=f8eb6e44-85ae-4e56-bc00-587876699945
runtime=/nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented
runtime_sha=31b46685045189ce6f8e73df2aaafb7944d86a5ff6c3d3f3782aaab4f58655a1
handoff_sha=1e7247c6e7e90a22fa41d022afb9e45001bf1328e69ec1440641f1c919518ee4
due=1793436900
wake=1793437860
window_end=1793484000
btime=1793381198
historical_btime=1793381192
success_before=10
markers_before=56
kind=s3
predecessor=/var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-completed.txt
predecessor_sha=2333c0af7c6fa351a53ad5202189df7b469963590245485cb386ae7f6b6227ed
pm_async=0
pm_trace=0
recorder=rtc-unsync-zerooffset
tpm=initialized
ec=not-run
measurement=dated-power-graph-required
reset_provenance=owner-recovery-mechanism-unspecified
nonce=rtc-s3-20261031-1793436900
PLAN
/nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented check /var/lib/predator-pm-instrumented/rtc-s3-20261031-plan.conf aa5be693e84beca952edddf18a7594169c3b08e3885bf61caaf4ca68d1ba5fa9
journalctl --sync
