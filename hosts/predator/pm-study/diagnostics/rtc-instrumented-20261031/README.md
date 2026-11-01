# RTC-instrumented calibration and conditional S3 trial, October 31

The owner followed the Pi-WoL discussion with **“do all then. test it”**. This
authorizes completing instrumentation preparation, testing it, and one guarded
real S3 attempt if that preparation succeeds. The earlier
[`rtc-feasibility-20261031`](../rtc-feasibility-20261031/README.md) assessment remains
an unchanged record of what had not yet been validated at 08:46.

## Current result

**Owner-confirmed15W hang, forced reboot, recovered11:09.** The [recovery report](trial-rtc-s3-20261031/recovered-1109/README.md) preserves the new boot's original RTC and old trial files. The retained marker conditionally identifies `acpi_suspend` start; no end marker or completed return is retained. RequestOk and clock quiescence are now verified. Newboot`ae29f675-9167-4975-a884-01885468bc38` is healthy, counters0/0, with frozen system/kernel/modules unchanged. The owner requested next tests; an11:45CET task will prepare and run one eligible instrumented s2idle control with Pi WoL, using a distinct reviewed protocol. It must announce actual entry/wake/review times before arming. No machine timer or new PM setting is applied by this scheduling turn. Recovery evidence and the deferred review are now available for guarded synchronization.

### Earlier review, before owner recovery

The **10:21 review** found Predator unreachable after the09:55 deep-suspend
entry and both scheduled Pi wake sends. The LAN probe failed; review SSH returned
`No route to host`. The [independent review](trial-rtc-s3-20261031/review-1021/README.md)
preserves the exact frozen capture, successful wake-send journals and failed
retrieval. No low-level return/exit was observed. Predator's own artifacts,
current counters/modes, lifecycle and health could not be read; physical sleep
or a hang has not been confirmed by a power graph. The owner was notified to
record watts/graph and recovery time/method/off duration. No extra wake, reset
or trial was issued. The exact review task was deleted, with no replacement.
New evidence is archived locally; remote documentation sync awaits recovery.

## Verified calibration and trial preparation

The calibration completed **09:35:19 CET** and passed independent review at
09:41–09:44. Its root0600 completion SHA256 is
`2333c0af7c6fa351a53ad5202189df7b469963590245485cb386ae7f6b6227ed`.
The kernel shows one additional ordered core checkpoint and exit, no low-level
resume and no sleep-duration accounting. Raw hardware RTC was
**2023-04-10 00:00:01 UTC**, action17/value3/startN, markers56, with STA_UNSYNC
still set. Fresh NTP synchronized at09:35:17; the subsequent RTC, LAN/DNS and full
health checks passed. Counters are10/0, ordinary modes restored, same boot.
Observed btime is now1793381198; retain both earlier readings1793381192 and
1793381193. This is clock-derived drift, not evidence of a reboot.

The review deleted its exact09:41 task before collection. Original artifacts,
their remote hashes, the independently checked frozen Pi result and a separate
read-only verifier are archived in [`calibration-review/`](calibration-review/).

One [real S3 trial](trial-rtc-s3-20261031/README.md) was armed **09:45:49–09:45:51**
for **09:55**, Pi wake **10:11**, retry10:11:30, probe10:12:30, snapshot10:14 and
automatic review **10:21**. The09:50 target moved to preserve setup lead. The
same R3 runtime pins calibration completion, actual btime and markers56. No
actual S3 result exists at arming. The owner received exact times and a request
for the dated09:53–10:15 power graph before entry.

## Calibration preparation history

One recorder-enabled `pm_test=core` calibration was armed at **09:29 CET** for **09:35 CET**,
with independent automatic review **09:41**. Its ID is
`rtc-cal-core-r3-20261031`. It stops before `suspend_ops->enter`, so it cannot count
as physical sleep. The Pi will make one bounded probe at 09:36:30 and freeze its
netconsole result at 09:38. No WoL is needed for this calibration.

Manifest SHA256 is
`d9994805b88f3a30454407f32d75e1d2bbaa08b975b9e86af1380b00b5e8b8b7`.
At09:29:04 the primary was inactive with no requested artifact, its one-shot
timer was waiting09:35, and its Type=exec/runtime bound/restore command matched
the manifest. This supplied over350seconds lead. Owner notice was delivered
before arming. The actual next cron timer was09:32:29, safely before09:35.

The preparation's conditional target was09:50, with prospective Pi wake10:06
and review10:16. Those times were not armed. The reviewed09:55 trial above is
the actual committed schedule.

The exact calibration review task is
`scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-rtc-core-review-20261031-0941`.
Its verified next run was `2026-10-31T08:41:00.000Z`, bound to this thread. It was
deleted at review entry; the date guard passed09:41:17. Machine actions remain
limited to October31,08:00–23:00 Europe/Paris. No polling or unconditional repeat
is authorized.

## Exact history and runtime transition

Boot remains `f8eb6e44-85ae-4e56-bc00-587876699945`. Before calibration there are
nine original diagnostic cycles, success/failure 9/0, and **zero actual S3
attempts** on this recovery boot. Pre-calibration observed btime was `1793381193`; the
historical record remains `1793381192`. Owner recovery mechanism and off duration
remain unspecified. Calibration adds a tenth diagnostic only. Its cumulative
RTC-marker count cannot be reset; any next manifest must pin the observed count
and clock-derived btime after restoration rather than relabeling this boot.

The new, separate binary is `predator-pm-instrumented`. Existing ordinary and
diagnostic runners and completion records retain their original semantics.
The new protocol writes only `/var/lib/predator-pm-instrumented`, with root0600
create-new manifests and artifacts. Its two operation kinds are
`calibration-core` and `s3`; neither creates an ordinary study PASS.

Validated runtime:

```
/nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented
binary SHA256 31b46685045189ce6f8e73df2aaafb7944d86a5ff6c3d3f3782aaab4f58655a1
source SHA256 947f45892c8c052553938d4df5e7dbfb36db83d367e7ffde76e14db4156dea87
```

The frozen system/kernel/modules/recorder identities are unchanged from the
feasibility report. The new Nix-built runtime is rooted locally and remotely,
without system activation. The unchanged clock-handoff helper is
`/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study`,
SHA256 `1e7247c6e7e90a22fa41d022afb9e45001bf1328e69ec1440641f1c919518ee4`.

Two earlier preparation builds remain retained. R1 was never deployed and had
its explicit lower date bound corrected. R2 was deployed/rooted, but its
read-only preflight rejected a hyphenated journal boot-ID argument at 09:23:35.
That manifest and failure are retained; **no PM operation, clock mutation or
timer was executed by R2**. R3 uses the journal's compact boot ID and passed the
full preflight at 09:25:32. An interrupted initial Nix build and transport/import
failures are also retained. Root import of the reviewed package succeeded; no
system trust or signature policy was changed.

## RTC writer barrier and calibration requirement

The matching Linux 6.18.58 `ntp.c`, `process.c` and `workqueue.c` establish the
following sequence:

1. Stop timesyncd; use `clock_adjtime` to set `STA_UNSYNC` and make a **zero**
   `ADJ_SETOFFSET` update. Verify the resulting status and stopped service.
   UNSYNC prevents future kernel RTC writes; the offset operation cancels the
   RTC-update hrtimer without intentionally changing realtime.
2. A writer that was already running might still overlap the early markers.
   `freeze_kernel_threads` freezes freezable workqueues and waits until their
   active work drains. `system_freezable_power_efficient_wq`, used by RTC sync,
   is covered. Device-suspend and later low-level markers are after this barrier.
3. Run serial core calibration with trace0 and the existing RTC recorder armed.
   Require a genuine core checkpoint/exit, no low-level S3 return, new markers,
   final action17/value3/startN, and hardware RTC **2023-04-10 00:00:xx UTC**.
   Volatile marker fields alone do not establish that the RTC write succeeded.
4. Only after the suspend service and all sleep jobs are inactive, preserve the
   raw RTC and marker fields, disarm the recorder, restore none/async1, and run
   the pinned clock handoff. Require fresh synchronized time and full health.

The calibration validates current readback/restoration. It cannot prove how
firmware will preserve a marker through every possible forced recovery. The
historical RTC records support retention conditionally; the original elapsed
time, device-hash, firmware-write and reset assumptions still apply. In a real
hang, preserve the next boot's early RTC capture before any interpretation.

## Sleep measurement and lifecycle

Pi WoL is independent of Predator's overwritten RTC. The kernel still loses its
usual sleep-duration sources under RTC tracing. A returning real trial is
therefore recorded as `INSTRUMENTED_RETURN_PENDING_POWER_PROOF`, not PASS.
Only ordered genuine entry/low-level return/exit, request acceptance, valid
timing, restored full health, and a dated graph measuring at least 900 seconds
of physical low-power sleep can establish full PASS. Neither the 960-second
schedule nor an NTP step is that measurement.

The primary uses Type=exec, RuntimeMaxSec=2400, bounded command/journal readers,
and safe ExecStopPost restoration. Readers are killed and reaped. Restoration
refuses active or queued sleep jobs and captures raw RTC before clock writers.
Preflight pins runtime/manifest/predecessor hashes, current boot/history/btime,
frozen identities, inhibitors, TPM CRB/raw-EC exclusions, AC/lid/fb/WoL, declared
LAN/DNS/NTP, pools/mounts/apps, no failed units, and maintenance exclusion.
Actual Nextcloud cron at 09:22:30–31 and next timer09:27:29 imply calibration09:35
is between expected09:32 and09:37 runs; execution checks maintenance again.

At 09:25:35 the independent Pi receiver was active, NTP synchronized, and the
matching nonce was present in its live stream. Frozen baseline SHA256 is
`84f653e96f1aaabafa90b900cf4dae75c0ed7179c014b93003fac3fd49c2e5e7`.
Pi boot is `2142644e-84db-40ae-b1b9-9e8c71265cf7`. Waker SHA remains
`ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`.

Eight focused protocol tests pass, including late kernel-exit delivery after
service completion, strict history, calibration/S3 separation, readback and
cleanup refusal. The R3 Nix build log confirms all46 package tests passed
(control2, diagnostic14, instrumented8, main22). Flake evaluation passed.
No hardware outcome is claimed by those software tests.

Evidence and the bounded operator source are retained under
`.scratch/pm-instrumented-execution-20261031/`; the reviewed preparation snapshot
is archived alongside this report. Failed attempts and prior immutable evidence
are preserved. No commits, pushes, delegates, reboot, firmware or security
change were used.
