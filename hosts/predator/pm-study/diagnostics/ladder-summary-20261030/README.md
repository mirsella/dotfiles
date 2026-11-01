# Completed phase-isolation ladder, October30

The owner's **"do that fully"** was executed through the bounded nine-return ladder. Freezer returned once, followed by devices, platform, processors and core twice each. Every operation used boot`f8eb6e44-85ae-4e56-bc00-587876699945`, btime1793381192=18:26:32CET. No intervening reboot, WoL, kernel/system activation, TPM/LUKS/storage or firmware change. Final core completed22:33:48 and was independently reviewed22:38.

The freezer's original LAN post-check failure and absent completed file remain intact. Its retained successful return/cleanup were separately revalidated at20:03:57, with a new actual-time record and explicit predecessor kind. Eight later operations have genuine original completed records. The oneshot self-job abort and cancelled first core preparation executed no cycles and are not counted. No old completion or physical sleep was synthesized.

## What this establishes

- Task freezing/unfreezing returned; device preparation/normal callbacks, late/noirq and platform preparation returned repeatedly.
- Both processors tests explicitly offlined CPUs7..1 and restored CPUs1..7. Both core tests exercised IRQ/syscore suspend/resume before unwinding.
- The two core captures show Realtek r8169 normal, late/noirq and resume callbacks returning0. The earlier final netconsole r8169 line cannot identify that driver as the hang cause.
- Core calls timekeeping syscore suspend/resume while skipping actual firmware entry. The recorded4.962s and4.373s are clock accounting, not physical S3. Even ACPI "Preparing to enter" and "Waking up" messages occur on this diagnostic path; those labels alone do not prove S3.
- Final counters9/0 and8035 independently boot-validated kernel records agree with the retained per-cycle captures. No low-level firmware resume was accepted. Ordinary S3 last-completed remains unchanged fromOctober18, SHA256`280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08`.

These observations focus the next experiment on actual S3 entry/power transition/return, but do not prove firmware is the culprit or exclude an intermittent failure in shared preparation paths. Diagnostic unwind differs from actual sleep and its return. Prior~15W hangs, priorQ35 owner3W/old verifier error and the30W shutdown/startup outage retain their separate classifications. This boot's diagnostic count is not an ordinary first-S3 trial or a D primer for E.

## Instrumentation and final state

Runtime changes were explicit and archived: bounded event-driven LAN readiness/audited freezer revalidation, then a narrow core clock-accounting correction with a SHA-pinned processors→core runtime handoff. All38 project tests, independent collector regression, Nix build and flake evaluation passed for the final packagecs0lilrdblqkd20835ihmlk61yf8nh0b; sourceSHA61143dbe75133ad76af9f57cd4962d652041237ef7ef558a2d52aca46bb55fd5/binarySHA25d8e1e0530e68c2ca20b35b7c2181a6518b3775c5f5ffe703b2e3871256f750. Kernel/system/modules stayed frozen. Keep every earlier runtime/system GCroot.

At22:38:36 the host remained healthy: pm_testnone, async1, trace0, recorderN/markers0, success9/fail0, inactive sleep/diagnostic units and no jobs/failed units. Expected LAN UUID/DNS and synchronized NTP, both healthy pools/five exact mounts, actual application services and four healthy Immich containers. No future PM timer on either machine. All direct journal/readiness followers were reaped. Raw return, observed journal, readiness, after-kernel and completed records remain separate; full Pi snapshots carry size/SHA proofs. Final completedSHA`7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc`.

Independent final read-only proof22:44:50–22:44:53 reconfirmed the same boot, CPUs0–7 online, counters9/0, normal modes, healthy network/NTP/pools/five mounts/containers, no jobs/failed units and no future study timer on either machine. The cancelled d-core-01 still has no requested artifact; original freezer completed still absent and its separate revalidated hash unchanged. The historical system scratch GCroot is present locally; that scratch path is absent remotely, where `nix-store --query --roots` explicitly confirms current-system, booted-system and system-generation167 roots for the same frozen system. All four additional runtime roots resolve remotely. Nothing was removed or substituted.

Closing evidence and verified schedule metadata are in `closing-evidence/`. Sanitized Telegram acknowledgement88820, epoch1793396751, reported the completed ladder, healthy state, evidence-preserving interpretation and next read-only preparation. No private notification values are archived. Guarded final synchronization proof is retained separately in `.scratch/pm-ladder-final-sync-20261030-2247/`.

## Next justified preparation

The useful next experiment is one supported real-S3 attempt with phase evidence that survives the network-console blind spot and owner recovery, rather than more age comparisons or blind driver changes. First assess the existing RTC phase recorder, pre-NTP boot preservation/decode and reset-retention assumptions. Do not assume a continuous pstore log or usable UART: current console/PMSG/ftrace pstore options are disabled, SPCR absent and legacy UARTs unknown. Reserved RAM/ramoops or firmware changes require a separate validated setup. No recorder/RTC setting, watchdog or crash trigger was enabled during this ladder.

The owner subsequently said **"schedule the next test"**. The same task was updated, without creating a duplicate, from read-only design to **preparation and guarded execution of one supported instrumented S3 attempt**, starting **October31 08:30 Europe/Paris**, verifiednextRunAt`2026-10-31T07:30:00.000Z`:
`scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-s3-logging-preparation-20261031-0830`.
It deletes itself even stale/early and enforces the dated08:30..23:00guard.08:30 is preparation start, not an already armed suspend time. It must first validate meaningful durable phase capture, current identity/history and full health; only then prepare one immutable instrumented trial with independent wake coverage and its dated self-deleting review. Existing RTC instrumentation is a candidate requiring review, not an assumed reliable logger. New standalone Rust protocol work must be tested and explicitly pinned; the frozen kernel/system and firmware/TPM/LUKS/security boundaries remain unchanged. No unsupported instrumentation, blind suspend, recovery WoL or forced reset is authorized. A healthy fresh-start preparation is permitted only when genuinely necessary, under the existing single-shutdown/single-planned-WoL rules; diagnostic history cannot be erased or relabeled to bypass a verifier.

The task must actually execute the eligible next experiment rather than stop at another read-only recommendation. If logging or health cannot meet its requirements, it must report the concrete blocker and leave the machine awake. No recurring polling task is queued. Any actual S3 trial still needs its full08..23window, ≥900s measured sleep, dated graph and exact boot/age/ordinal/reset/runtime evidence. The original read-only schedule in `closing-evidence/` is immutable historical evidence; [schedule-amendment-20261030.json](schedule-amendment-20261030.json) records the later enabled task scope. This amendment updates local task metadata only; remote documentation is to be synchronized with hash guards during the scheduled preparation.

## October31 preparation outcome

The task passed its date guard at08:30:22 and deleted itself. The
[RTC instrumentation assessment](../rtc-feasibility-20261031/README.md) stopped
before arming because the recorder disables both RTC sleep-duration readers,
the host has no advertised S3-nonstop clocksource, and no independent validated
measurement or complete writeback/retention protocol was established. Five
offline decoder tests passed. Two older retained boot clocks are conditionally
compatible with ACPI-S3 start markers, but their older kernel/reset provenance
does not establish a culprit for the current baseline.

At08:46 the same boot was healthy and awake, counters9/0, ordinary modes and
markers, with zero actual S3 attempts on this boot and no future study timers.
The one-second btime drift to1793381193 is recorded without rewriting the
October30 reading. Scheduler verification found no remaining Predator task.
No RTC/PM setting, runtime, system or power transition changed. The next test
is blocked on validated measurement and durable capture, with no blind repeat
or polling task. Original closing evidence and schedule amendments remain
immutable; the October31 assessment records the consumed task and outcome.
