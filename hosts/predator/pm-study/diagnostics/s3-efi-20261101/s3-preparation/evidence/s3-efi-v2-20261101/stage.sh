set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$h" -ge 8; test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = 39d7579b-b86d-4d30-b910-131943e550c8
date --iso-8601=ns
test -z "$(systemctl list-jobs --no-legend)"
test -z "$(systemctl list-timers --all --no-legend | grep -E 'predator-pm-(boundary|path|wake|diagnostic|instrumented|s2idle)-' | grep -v '^-' || true)"
install -d -m700 /var/lib/predator-pm-efi-s3-reviewed
umask 077
set -o noclobber
cat > /var/lib/predator-pm-efi-s3-reviewed/core-control-review.json <<'CORE_REVIEW'
{
  "contextFlags": 7,
  "counterAfter": 1,
  "counterBefore": 0,
  "independentVerification": "PASS",
  "mode": "core",
  "originalCompletionAbsent": true,
  "originalHarnessResult": "POST_CLEANUP_EXACT_BTIME_GUARD_FAILURE",
  "originals": 16,
  "physicalSleepSeconds": null,
  "piFrozenHashesAndPrefix": "PASS",
  "piProbeAndSnapshot": "PASS",
  "priorKernelRecordsPreserved": 1162,
  "rawEpoch": 1793537120,
  "rawRtcEpoch": 1793537121,
  "realS3": false,
  "recordPhase": 11,
  "recordSequence": 2,
  "remainingFree": 7034,
  "reportedStorageCost": 308,
  "timestamps": [
    {
      "message": "PM: suspend entry (deep)",
      "microseconds": {
        "_SOURCE_BOOTTIME_TIMESTAMP": 2031857672,
        "_SOURCE_MONOTONIC_TIMESTAMP": 2031857672,
        "__MONOTONIC_TIMESTAMP": 2031913543,
        "__REALTIME_TIMESTAMP": 1793537110492030
      }
    },
    {
      "message": "pm_boundary_stage: begin nonce=core1101a phase=10 sequence=1 context=7",
      "microseconds": {
        "_SOURCE_BOOTTIME_TIMESTAMP": 2033707416,
        "_SOURCE_MONOTONIC_TIMESTAMP": 2033707416,
        "__MONOTONIC_TIMESTAMP": 2037269554,
        "__REALTIME_TIMESTAMP": 1793537120509309
      }
    },
    {
      "message": "pm_boundary_stage: end nonce=core1101a phase=10 sequence=1 error=0 efi=0 fast_clock_delta_ns=0",
      "microseconds": {
        "_SOURCE_BOOTTIME_TIMESTAMP": 2033755209,
        "_SOURCE_MONOTONIC_TIMESTAMP": 2033755209,
        "__MONOTONIC_TIMESTAMP": 2037269592,
        "__REALTIME_TIMESTAMP": 1793537120509347
      }
    },
    {
      "message": "pm_boundary_stage: begin nonce=core1101a phase=11 sequence=2 context=7",
      "microseconds": {
        "_SOURCE_BOOTTIME_TIMESTAMP": 2038741222,
        "_SOURCE_MONOTONIC_TIMESTAMP": 2038741222,
        "__MONOTONIC_TIMESTAMP": 2037269658,
        "__REALTIME_TIMESTAMP": 1793537120509413
      }
    },
    {
      "message": "pm_boundary_stage: end nonce=core1101a phase=11 sequence=2 error=0 efi=0 fast_clock_delta_ns=0",
      "microseconds": {
        "_SOURCE_BOOTTIME_TIMESTAMP": 2038785429,
        "_SOURCE_MONOTONIC_TIMESTAMP": 2038785429,
        "__MONOTONIC_TIMESTAMP": 2037269697,
        "__REALTIME_TIMESTAMP": 1793537120509453
      }
    },
    {
      "message": "PM: suspend exit",
      "microseconds": {
        "_SOURCE_BOOTTIME_TIMESTAMP": 2042222071,
        "_SOURCE_MONOTONIC_TIMESTAMP": 2042222071,
        "__MONOTONIC_TIMESTAMP": 2037397359,
        "__REALTIME_TIMESTAMP": 1793537120637114
      }
    }
  ],
  "schema": "core-transport-independent-review-1",
  "result": "CORE_TRANSPORT_INDEPENDENTLY_VERIFIED",
  "boot": "39d7579b-b86d-4d30-b910-131943e550c8",
  "successAfter": 1,
  "cleanupVerified": true,
  "fullHealthVerified": true,
  "healthAtLocal": "2026-11-01T13:54:10+01:00",
  "btimeBefore": 1793535078,
  "btimeAfter": 1793535079,
  "originalFailurePreserved": true,
  "failedUnitStateClearedWithoutRepeatingCleanup": true
}
CORE_REVIEW
echo '78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896  /var/lib/predator-pm-efi-s3-reviewed/core-control-review.json' | sha256sum -c -
cat > /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-plan.conf <<'PLAN'
schema=efi-s3-reviewed-20261101-2
id=s3-efi-v2-20261101
nonce=deep1101a
boot=39d7579b-b86d-4d30-b910-131943e550c8
btime=1793535079
mode=s3
due=1793539200
rtc=1793539500
review=1793539800
runtime=/nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3
runtime_sha=810564d7fc6d6ab3331220ce07e9c9a9959debde3d8299179c106881bef23e3b
source_sha=5c0e3e70286d63c60d73373239fc284a1872b11c63e6df0c7304f0f70a20c7bb
module=/nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz
module_sha=90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0
success_before=1
core_review_sha=78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896
retention_sha=176cbbc62c46cd68c977a07d6c3f775d770ff0983176190646c17f9c18d37b63
owner_ready=owner-confirmed-ready-20261101
pm_async=0
pm_trace=0
pm_test=none
PLAN
/nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3 check /var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101-plan.conf bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c
journalctl --sync
