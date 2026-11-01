# One RTC-instrumented s2idle control, October 31

Current result after evening recovery: [original host artifacts and analysis](recovered-2058/README.md) prove that s2idle returned on IRQ9/ACPI notify with lid-open and short-power-key events, then Linux powered off normally. Restoration was interrupted by shutdown. The owner reported “3-1w”; this is not the earlier15W S3 hang. The scheduled WoL sends did not establish a return. Source kernel timestamps span14558.172999s in the loop; the old0.000121s result used journal receipt timestamps and is invalid as elapsed-loop timing. Also, s2idle does suspend timekeeping through cpuidle/tick_freeze, despite bypassing the deep/syscore branch. The RTC logger prevented normal clock recovery; do not label the old uncorrected return epoch as early wake or reuse the old no-Timekeeping-message guard.

Newboot`f9b45aa6-b820-4ab5-b18c-9324d8d53087` is healthy after clock-handoff repair. The tested event-driven readiness fix is rooted for the next protocol. One new workflow is scheduled21:45CET for a recorder-off s2idle WoL test with a verified RTC-alarm fallback; exact timing awaits validation, with review before23:00. No machine operation is armed. The following12:33 assessment and preparation are historical; their immutable evidence is unchanged.

Current state at12:33: unreachable after s2idle entry and both scheduled Pi WoL
sends. The [read-only review](review-1233/README.md) verified the frozen observer
evidence but could not retrieve host artifacts. No return was observed; a hang
and failure to wake via WoL remain unresolved. The owner was asked for watts,
the dated power graph and manual recovery details. The review task deleted itself;
no additional power operation or repeat was scheduled. Evidence is local until
Predator recovers and remote preimages can be checked again.

The following records the completed preparation. The owner's “force rebooted it,
it was at 15w. schedule next tests” authorized one useful next control. The dated
11:45 workflow deleted its exact task, passed the date guard at11:45:23CET and
prepared a distinct runner. Arming completed11:55:21.714962797CET, with more than
698 seconds before entry. No additional sleep transition was used for calibration.

## Schedule and immutable plan

- ID `s2idle-20261031`; entry October31 **12:07CET**, epoch1793444820.
- Pi wake **12:23**, epoch1793445780; planned retry12:23:30.
- Bounded LAN probe12:24:30; frozen netconsole snapshot12:26.
- Automatic read-only review **12:33**, verified nextRunAt
  `2026-10-31T11:33:00.000Z`.
- Exact enabled, current-thread-bound review task:
  `scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-s2idle-review-20261031-1233`.
  It first reads `review-schedule.json`, deletes that exact task, and enforces
  October31 and the08:00–23:00 Europe/Paris window. It authorizes no second power
  transition or automatic deep retry.
- Root0600 manifest
  `/var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf`, SHA256
  `f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820`.
- Exact times and request for a dated12:05–12:27 power graph were sent before
  arming. `owner-notified.txt` contains the sanitized delivery acknowledgment.

The consumed preparation task was
`scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-s2idle-control-20261031-1145`.
Its11:45 time was the workflow start, not a sleep deadline.

## History and reason for this control

Recovery boot `ae29f675-9167-4975-a884-01885468bc38` had zero diagnostic, S3 or
s2idle attempts at preflight, counters0/0 and recorder markers0. Observed and
historical btime1793440484 is10:54:44CET; it is a clock-derived boot start, not
the owner's button timestamp. Planned age at entry is4336seconds,1h12m16s.
The exact forced-reboot method and off duration remain unspecified.

Old boot `f8eb6e44-85ae-4e56-bc00-587876699945` retains ten diagnostics followed
by one failed real-S3 request. The owner confirmed15W and forced recovery.
[Recovered evidence](../rtc-instrumented-20261031/trial-rtc-s3-20261031/recovered-1109/README.md)
includes requestOk, quiescence and the early RTC marker, not a retained return.
Root0600 recovery RTC SHA256 remains
`b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55`.
Its conditional ACPI S3 entry decode narrows a boundary, not a culprit.

This control exercises s2idle instead of firmware S3. It is outside the original
nine-cycle ladder and the later RTC-core calibration. It cannot be an S3 PASS or
an ordinary D/E/Q primer. A return would show this path can unwind on this boot,
not prove the intermittent deep-suspend problem absent or isolate a single driver.

## Explicit new runtime

New source `pkgs/predator-pm-study/s2idle.rs`, bin `predator-pm-s2idle`, schema
`rtc-s2idle-control-1`, state `/var/lib/predator-pm-s2idle`. Original R3
`instrumented.rs`, `diagnostic.rs` and all existing runtimes/roots remain intact.
No old guard was loosened, completion replaced or prior cycle fabricated.

- Package `/nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0`.
- Binary SHA256
  `27c4152e2577d3569027fa479a412cb08e63f41a63d99755b45a6e17eaad5a33`.
- Source SHA256
  `618ecaee583e6b2a2a1489649418731ddbfcad3c65f2e9d109772f4c6db602df`.
- Local and remote GCroot `.scratch/pm-s2idle-control-20261031/runtime-root`.
- Bounded root import/hash verification completed11:52:26. No system activation,
  trust-policy change, persistent sleep configuration edit or frozen baseline change.
- Scratch operator source SHA256
  `afa4c2e1a4616615cbedb253dfd1a66527c188fb17b4cd71258501f30d7628fc`;
  compiled operator SHA256
  `f5824d407c9eae10c836abd7273be28e6bae0dd84e6242dad5129151cc5e59ef`.

All55 package tests passed: control2, diagnostic14, existing instrumented8,
new s2idle9, main22. New tests reject wrong boot/history/modes/measurement,
systemd overrides, deep or diagnostic results, missing/out-of-order loop return,
incorrect retained RTC marker and unsafe cleanup. They also cover delayed kernel
exit delivery, early/late classification and the Timex ABI. Nix build and
`nix flake check . --no-build --no-update-lock-file` passed. These are software
checks, not a hardware result.

Frozen system is
`/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`;
kernel `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`;
modules `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
Recorder remains
`/nix/store/3jcksrsfmyw8vk1cwp0x5ynb3h1q1pdp-predator-pm-stage-20261009/lib/modules/6.18.58/extra/pm_stage.ko.xz`,
SHA256`8fd79959fa979cabf2dfac97c4a60dbdab505cca5cf3e3902772faebd3071d9e`.
Pinned clock helper remains
`/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study`,
SHA256`1e7247c6e7e90a22fa41d022afb9e45001bf1328e69ec1440641f1c919518ee4`.

## Kernel, systemd and RTC semantics

Matching Linux6.18.58 `kernel/power/suspend.c` branches to the s2idle loop after
device late/noirq and platform preparation, before CPU offlining, syscore/timekeeping
suspend or `suspend_ops->enter`. It emits `PM: suspend-to-idle` and
`PM: resume from suspend-to-idle`; `machine_suspend` tracepoints may surround
individual idle waits. It does not call the ACPI S3 low-level entry routine.

The final `console_resume_all` tracepoint carries state1, not S3's state3.
Expected retained final marker is action17/value1/startN. With device hash0,
its RTC calendar is **2021-04-10T00:00:xxUTC**, not the S3 final2023 date.
The existing module's unchecked RTC write and separate century write still
require actual readback. A source-derived expectation is not a claimed capture.

Effective systemd262 sleep configuration at11:46 was exactly `[Sleep]`,
`AllowHibernation=false`, `AllowSuspend=true`, with no MemorySleepMode or
SuspendState override. Matching systemd source initializes the memory-mode list
empty; `write_mode` does nothing for an empty list, preserving explicit sysfs
selection. The runner rejects any additional effective setting, unit drop-in or
changed frozen systemd-suspend unit. It checks configuration again after selecting
`mem_sleep=s2idle` and verifies `[s2idle] deep` before requesting suspend.

Execution uses serialasync0, trace0, pm_testnone and the existing RTC recorder.
It stops timesyncd, sets STA_UNSYNC and zero ADJ_SETOFFSET to cancel the future
kernel RTC timer. The already-reviewed freezer/freezable-workqueue barrier drains
an in-flight RTC writer before meaningful late markers. Matching sources and
previous calibration evidence remain retained.

After return, raw RTC/marker/status capture precedes disarming or clock writers.
Safe restore requires systemd-suspend inactive, MainPID0 and no queued sleep job.
Only then does it restore deep/none/async1/trace0/recorderN and invoke the pinned
LAN/fresh-NTP/RTC handoff. Cumulative markers are never reset. An active/unknown
sleep state prevents restoration rather than changing the selected mode underneath it.

## Health, observer and lifecycle

Fresh health and both11:53/11:55 pinned preflights verified frozen identities,
all five authenticated boot/late-phase rejection audits, TPM CRB/rawECnot-run/Acer
exclusions, AC/lidclosed/fbblank4/WoLg, declared LAN UUID
`c4e6863b-7f76-4a4d-9a48-9937af0b5a36`/DNS192.168.1.1/NTP, pools/five exact
mounts/apps, no HOLD/jobs/failed units/builders/active maintenance, zero prior
cycles and ordinary PM settings. No reset or power operation occurred during prep.
Actual cron11:49:49–50, then11:54:49; next at arm11:59:49. Expected12:04:49
and12:09:49 bracket entry12:07;12:24:49 follows wake12:23. Execution rechecks
maintenance rather than relying only on that forecast.

Pi proof11:53:48: boot`2142644e-84db-40ae-b1b9-9e8c71265cf7`, receiver active,
NTP synchronized to195.154.185.215, normal leap/stratum2, offset+2.272ms,
delay8.985ms, jitter2.067ms, packet595. Exact live nonce
`S2IDLE READY id=s2idle-20261031 nonce=s2idle-20261031-1793444820` was present.
Frozen baseline SHA256
`a767d259d642d52e1dbebc881fd7bbf58e11f6adee27f2a7ff39f7f2d8d6cfa2`
was independently retrieved and matched. Pi waker SHA256
`ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`
was rechecked before arming.

Pi timers armed11:55:13; primary armed11:55:14–21, with primary inactive and
no requested artifact at verification. Primary unit
`predator-pm-s2idle-s2idle-20261031` uses Typeexec, RuntimeMax2400seconds,
TimeoutStart30, TimeoutStop180 and pinned safe ExecStopPost. Pi units use Typeexec,
RuntimeMax30 and one-second timer accuracy. No polling loop or unconditional repeat.

## Review rules and evidence

Run scratch `operator collect DIRECTORY ID` once, retaining bounded failures.
Verify frozen Pi hashes, nonce and actual wake/probe/snapshot journals independently.
Retrieve root0600 originals and require exactly one s2idle entry, its loop/return,
exit and systemd completion. Deep entry, diagnostic checkpoint, ACPI low-level
return or timekeeping-suspend messages are incompatible with this control.
Expected successful counter is1/0; actual cumulative markers must be recorded,
not guessed. Preserve clock-derived btime drift.

Completion may be `S2IDLE_EARLY_RETURN`, `S2IDLE_LATE_RETURN` or
`S2IDLE_RETURN_PENDING_POWER_PROOF`. Early return is not a16-minute success.
Kernel monotonic loop duration is recorded but not equated with measured physical
low-power duration. Schedule960seconds, NTP correction and unreachability are also
not physical proof. S2idle can draw more than S3's usual3W;15W alone is not a hang
if the machine returns. The owner's dated power graph is still needed.

On an unreachable host, retain bounded observer evidence and request owner
watts/graph/recovery. Do not infer absent artifacts from failed retrieval, issue
extra WoL/reset/watchdog/crash actions, kill a running primary or remove wake
coverage without proven request cancellation. No further power transition is
authorized by the read-only review.

`preparation/` preserves the dated workflow outcome, source/runtime/operator pins,
software verification, matching sources, fresh health/config inspection, plan,
independent Pi capture, scheduler receipt, owner notice and arming evidence.
Its SHA256SUMS covers the immutable preparation snapshot. Existing historical
archives and missing original STATE/PLAN remain unchanged.
