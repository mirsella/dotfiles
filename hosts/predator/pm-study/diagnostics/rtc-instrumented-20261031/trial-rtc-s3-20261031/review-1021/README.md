# Instrumented S3 review, October 31 at 10:21 CET

**UNREACHABLE_AFTER_S3_ENTRY_AND_SCHEDULED_WOL.** The independent Pi capture
shows one real-S3 request reaching the kernel's deep-suspend sequence. Both
planned wake sends ran, but the LAN probe failed and Predator was unreachable
at review. No successful return, physical sleep duration or physical hang state
is established. Owner watts and the dated power graph have not been supplied.

## Observed sequence

- Review deleted the exact task named in the original `review-schedule.json`:
  `scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-rtc-s3-review-20261031-1021`.
  October31,08:00–23:00 date guard passed **10:21:20 CET**.
- The unchanged `operator-r3 collect` ran **once**. Pi collection succeeded at
  10:21:21; Predator SSH failed with exit255, `No route to host`, before its
  remote collection script ran. Primary stdout is empty. This does not prove
  the requested/raw-return/outcome/completed files are absent.
- Pi received this trial's exact BEGIN at **09:55:02**, kernel monotonic
  55708.719300, boot`f8eb6e44-85ae-4e56-bc00-587876699945`, kindS3,
  success_before10, markers_before56, actual age55709seconds since the original
  boot and planned_wake1793437860. The history remains ten diagnostics followed
  by one real-S3 attempt; physical firmware entry is not independently proven.
- `PM: suspend entry (deep)` at monotonic55708.931939, received09:55:02.
  The freezable-task barrier completed at55709.449400. The final retained line
  is `r8169 ... calling pci_pm_suspend` at55710.269890, received09:55:03.
  This is the network logging boundary, not evidence that this callback hung.
- Pi logged WoL transmission to98:29:a6:3a:a7:50 via192.168.1.255:9 at
  **10:11:00.650997** and **10:11:30.644462**. Both send services completed
  successfully. Sending a packet does not prove Predator received or acted on it.
- The scheduled LAN SSH-keyscan probe started **10:12:30.552599** and exited1
  at10:12:35.586759. The no-clobber snapshot completed **10:14:00.563874**.
- The current-trial capture contains no `Low-level resume complete`, suspend
  exit or RTC RETURN. It also contains no diagnostic checkpoint or measured
  sleep time. Lack of network capture beyond the NIC boundary cannot locate
  the hardware stop or exclude an unobserved return with a networking failure.

## Independent evidence

The separately retrieved frozen file matches the collector's exact bytes and
the Pi's SHA256; its initial2,736,283bytes match the original frozen baseline.
The read-only verifier also checks the nonce, unique BEGIN, ordered entry and
freezer completion, actual wake journals, failed probe and retrieval failure.

- Baseline SHA256:
  `d9acaf48a67f432635116320aa1dae036f03edf218e788605fcee8508a246b40`.
- Frozen result:2,766,069bytes, mtime10:14:00.559691580, SHA256:
  `794c318ad424206e7b6fadd56df0a24ca601c4bd8857b4250e4df5926400cb57`.
- Manifest SHA256 unchanged:
  `aa5be693e84beca952edddf18a7594169c3b08e3885bf61caaf4ca68d1ba5fa9`.
- R3 binary/source and original diagnostic source hashes still match their
  reviewed pins. No source/runtime or frozen baseline change was made.

At10:22:11 the Pi retained boot`2142644e-84db-40ae-b1b9-9e8c71265cf7`, active
receiver PID1974 startedOctober17, synchronized NTP (offset−59µs, delay9.357ms,
normal leap, packet593), unchanged waker hash and no jobs. This trial has no
future Pi timer; the nine older notice timers are spent. The failed probe is
explained and retained. No assertion is made about Predator's current timers
or primary process, which could not be inspected.

The verifier initially named nonexistent `baseline.log`; the actual operator
artifact is `baseline.out`. That local verification failure/source is preserved.
After correcting the path, all independent evidence assertions passed. This did
not rerun the collector or any machine operation.

## What remains unknown

Predator's current boot, success/fail counters, cumulative RTC markers, recorder
state, PM modes, primary lifecycle, requestOk, clock restoration and full health
could not be checked. Their pre-entry pins are not post-return observations.
Neither expected11/0 nor expected116markers is asserted. No raw RTC was read and
no clock write or restoration was attempted while sleep state was unknown.

This is not `INSTRUMENTED_RETURN_PENDING_POWER_PROOF`, because a technical return
itself has not been established. It is not PASS. Scheduled960seconds, elapsed
unreachability and earlier diagnostic returns do not measure physical sleep.

## Owner recovery and next evidence

A Telegram notification was acknowledged during review, requesting current
watts, the dated09:53–10:15 graph, manual recovery and its exact time, method and
off duration. No recovery action has yet been reported or observed.

After owner recovery, preserve the new boot's existing root0600
`/var/lib/predator-pm-study/<new-boot-id>-boot-rtc.txt` before interpretation,
including its raw early kernel RTC reading and Magic lines. The existing
preserve-RTC service precedes NTP; do not replace the artifact with a later
corrected clock reading. Preserve the actual new boot identity, old trial
artifacts and journals, prior source/runtime roots, and firmware/reset history.
An RTC-derived future2099 timestamp must not be mistaken for the trial date.

Use the tested calendar decoder under explicit device-hash0, continuous ticking,
bounded elapsed recovery time and no intervening firmware/clock-write assumptions.
Naive Magic decoding after elapsed time can be misleading. Do not assign a phase
until the actual raw recovery value and uncertainty interval are available.
An ACPI-entry marker would narrow a boundary, not identify a driver or prove
whether failure was entering, remaining in or leaving S3.

This review authorized no new S3 trial, recovery WoL, reset, watchdog, crash
trigger or polling task. No existing primary was killed and no wake coverage
was removed. Exact review-task deletion was the only scheduler mutation.

## Archive and synchronization

`evidence/` contains collector scripts/output/status, separately retrieved frozen
capture, Pi proof, verifier/source/output, owner notification and schedule outcome.
Original preparation evidence is unchanged; the previous mutable trial README
and checksum manifest are retained here before the status update.

The archive and updated status documentation are local. Predator synchronization
is blocked by the single failed connection, not retried. Expected preimages
from the last verified09:48:49 sync are retained in
`.scratch/pm-instrumented-review-sync-20261031-1021/`; they are not a fresh remote
inspection. After recovery, inspect remote Git status/flake.lock and those
preimages before copying only the intended review/status paths. Unrelated work
and all historical artifacts remain untouched. No commits or pushes were made.
