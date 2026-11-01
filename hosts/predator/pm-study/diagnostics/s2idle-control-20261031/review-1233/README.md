# S2idle control review, October 31 at12:33 CET

Classification: `UNREACHABLE_AFTER_S2IDLE_ENTRY_AND_SCHEDULED_WOL`.
The kernel selected s2idle and began suspending devices. Both scheduled Pi wake
packets were sent, but the LAN probe and review SSH failed. No return is visible
in the frozen capture. This does not distinguish a hang from s2idle failing to
wake via WoL, or establish the machine's physical power state or sleep duration.

## Scope and collection

The review first read the trial's `review-schedule.json` and successfully deleted
exact task
`scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-s2idle-review-20261031-1233`.
The October31/08:00 to23:00 Europe/Paris guard passed at12:33:26CET.
The pinned operator/source checks passed and `operator collect` ran once.

Pi collection succeeded at12:33:31. Predator SSH exited255 with `No route to host`
before executing its remote script; its stdout is empty. No original host artifact,
current boot ID, counter, RTC marker, mode or service lifecycle was retrieved.
Failure to retrieve an artifact does not establish its absence.

The manifest remains SHA256
`f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820`.
The binary/source hashes and unchanged R3 instrumented/diagnostic source hashes
were verified locally and retained in `runtime-verification.txt`.

## Independent observer evidence

The frozen result was retrieved separately and matched the collector's bytes.
The standalone read-only Rust verifier confirms that it starts with the entire
previous baseline, then isolates only this trial's new data. It verifies the
manifest, nonce, single BEGIN/entry, ordered freezer boundary, final received line,
actual Pi service events and host retrieval failure.

- Baseline:2,882,530bytes, SHA256
  `a767d259d642d52e1dbebc881fd7bbf58e11f6adee27f2a7ff39f7f2d8d6cfa2`.
- Result:2,912,202bytes, SHA256
  `c41b683160c76853729a1de6bd461c703adecd3512abeb05b7eec9a056e5262e`.
- Exact nonce:`s2idle-20261031-1793444820`.
- BEGIN at kernel monotonic4345.572857, received12:07:10, identifies
  boot`ae29f675-9167-4975-a884-01885468bc38`, kind`s2idle-control`, prior successes0,
  markers0, planned wake1793445780 and age_since_recovery_boot4346seconds.
- `PM: suspend entry (s2idle)` at4345.785231, received12:07:10.
- Remaining freezable tasks completed at4345.853031, elapsed0.001seconds.
- Last received line, at4346.622581/12:07:11:
  `r8169 0000:03:00.1: PM: calling pci_pm_suspend @ 33540, parent: 0000:00:1c.3`.

That last NIC line is the network logging boundary, not proof of the failing
callback. The current-trial capture has no `PM: suspend-to-idle`,
`PM: resume from suspend-to-idle`, `PM: suspend exit` or `S2IDLE RETURN`.
It also contains no deep entry, diagnostic checkpoint, ACPI low-level resume or
timekeeping-suspend message. Messages emitted after the NIC stops transmitting
could still exist on Predator; reaching the idle loop is unverified here.

Pi journals separately establish:

- WoL sent12:23:00.644935; service completed successfully12:23:00.660040.
- Retry sent12:23:30.654975; service completed successfully12:23:30.674409.
- LAN keyscan started12:24:30.560617; exited1 at12:24:33.638776.
- Frozen snapshot completed successfully12:26:00.583634.
- No timer for this trial remained at review.

At12:34:16 Pi boot remained`2142644e-84db-40ae-b1b9-9e8c71265cf7`, receiver
active/running, NTP synchronized to195.154.185.215, normal leap/stratum2,
offset+697microseconds, delay9.212ms, jitter2.270ms, packet597. No jobs were running.
Its nine older notice timers were spent, with no future study timer listed.
The waker SHA256 remained
`ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`.

## Unresolved host outcome

This boot had zero cycles before this one observed s2idle request. The previous
boot's ten diagnostics and owner-confirmed failed S3 attempt remain unchanged.
Expected success1/0 and the source-derived final RTC2021-04-10/action17/value1/N
are not asserted as current observations. Actual cumulative markers and btime,
root0600 requestOk/return/outcome/restored/completed records, full kernel history,
RTC readback and safe restoration still require authenticated retrieval.

No technical return, early return, late return or passing physical interval can
be established. Scheduled960seconds, elapsed unreachability and NTP cannot measure
physical sleep. The dated12:05 to12:27 graph and current watts have not been
provided. S2idle can draw more than S3's usual3W;15W alone does not establish a hang.
The shared unreachable symptom does not prove a shared fault with firmware S3.

The review issued no extra WoL, reset, suspend, runtime/configuration change,
clock write or restore command. Sleep-job state was unknown, so neither mode
restoration nor cancellation of the existing primary was attempted. It scheduled
no polling task, automatic retest or deep trial.

## Owner recovery and preservation

Telegram delivery was acknowledged. The notice requests current watts, the dated
power graph, and manual recovery if the machine remains unresponsive, including
the exact time, method and off duration. A recovery has not yet been reported.

After owner recovery, preserve the new root0600
`/var/lib/predator-pm-study/<new-boot-id>-boot-rtc.txt`, its early raw RTC/Magic,
old trial originals and exact boot journals before interpretation. The existing
preserve-RTC service precedes NTP. A corrected current RTC is not a substitute for
that early artifact. Keep the earlier recovery artifact
`b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55` intact.

Decode the new marker using actual s2idle state1 and the tested calendar arithmetic,
with explicit devicehash0, continuously ticking RTC, bounded elapsed time and no
intervening firmware/clock-write assumptions. Do not assume the S3 state3 dates,
interpret a naive delayed Magic match as a culprit, or mistake encoded future
timestamps for the actual trial date.

## Archive and synchronization

`evidence/` preserves collector scripts/results/status, separately retrieved frozen
bytes, Pi health/proof, verifier source/output/build hashes, schedule outcome and
sanitized owner notification. The preparation snapshot/checksums remain unchanged.

This review and status updates are local. Predator synchronization is blocked by
unreachability; the SSH failure was not retried. The three expected documentation
preimages in `.scratch/pm-s2idle-review-sync-20261031-1233/` come from the verified
11:59:36 synchronization, not a fresh remote inspection. After recovery, inspect
remote Git status, flake.lock and those preimages before copying only the intended
new review directory and three updated READMEs. Preserve unrelated work, all
runtime GCroots, immutable earlier evidence and missing historical STATE/PLAN.
