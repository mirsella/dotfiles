set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=ns

test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != "-" {print}')"
systemctl list-timers --all --no-pager nextcloud-cron.timer
systemctl show nextcloud-cron.service -p ActiveState -p ExecMainStartTimestamp -p ExecMainExitTimestamp
systemctl list-jobs --no-legend
install -d -m0700 /var/lib/predator-pm-wake-path
umask 077
set -o noclobber
cat > /var/lib/predator-pm-wake-path/path-d3-20261101-plan.conf <<'PLAN'
schema=s2idle-wake-path-20261101-first-1
id=path-d3-20261101
boot=f9b45aa6-b820-4ab5-b18c-9324d8d53087
runtime=/nix/store/qar5vnhq2bk1vs3rr7g1z8kwa31bnwn4-predator-pm-study-0.1.0/bin/predator-pm-wake-path
runtime_sha=b58d2b0e7334bc443c6d801d6f5c5a2c062c17ad88a5b0ed9f33f6bdd8eecd02
source_sha=23f1cbee6a1b5c0c197b29a2d1ab5cb20125a2da55589de36ae32bb52940f7ec
timing_sha=d84db2b41c3f89279938dcc959690d5f41e6a619848dfebeb3864ca31d9f4648
handoff_sha=b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b
due=1793520900
wake=1793521860
rtc_fallback=1793522100
review=1793525400
window_end=1793570400
btime=1793474520
historical_btime=1793474520
success_before=1
markers_before=0
kind=s2idle-wake-path
recovery_sha=ec6d9a401a59064512c56c035894715d4123ef69ee301cc83297a373c9a83b7d
pm_async=0
pm_trace=0
pm_test=none
mem_sleep=s2idle
recorder=off-normal-utc-rtc-alarm
tpm=initialized
ec=not-run
measurement=dated-power-graph-required
reset_provenance=owner-forced-reboot-method-off-duration-unspecified
nonce=path-d3-20261101-1793520900
variable=nic-d3cold-allowed
original=1
intervention=0
rollback=1
hypothesis=upstream-bridge-D3-may-break-firmware-owned-PME-wake
prediction_wol=D0-bridge-and-PME-armed-with-WoL-return-supports-power-path-sensitivity
prediction_rtc=D0-bridge-and-PME-armed-with-RTC-only-return-shows-this-restriction-insufficient
instrumentation=six-pinned-existing-PCI-ACPI-debug-sites-original-off
PLAN
/nix/store/qar5vnhq2bk1vs3rr7g1z8kwa31bnwn4-predator-pm-study-0.1.0/bin/predator-pm-wake-path check /var/lib/predator-pm-wake-path/path-d3-20261101-plan.conf d61d7b6686710bae2451f5a7525c5f19864d936d4bb56cb20704de3cf3c76ce3
journalctl --sync

