# November 1 wake-path tests

## Reviewed result and closed second slot

The [10:30 independent review](review-1030/README.md) verified all24 originals and one new same-boot s2idle cycle, cumulative2/0/markers0. Return occurred at09:35 in the RTC-fallback window; neither09:31 WoL send produced a recorded return. Actual bridgeD0, NICD3hot and PME/ACPI arming were verified, so that restriction was insufficient. Cleanup restored NICd3cold1, all six debug sites off and ordinary PM/RTC settings. Core health/security/services passed; tank scrub was active at10:32 and blocked the observed maintenance preflight.

Static firmware mapping identifies GPE0x69 and a notification through sibling PXSX; its cumulative counter remained0 at10:39. This narrows the unresolved routing boundary without establishing another safe, source-supported setting change. The async-confirmation branch requires a verified WoL improvement and is not applicable. **Second slot NOT_ARMED:** no speculative toggle or unchanged repeat, no second transition, no further power workflow. Both scheduled tasks deleted their exact IDs before machine access. The historical preparation and protocol below remain preserved.

## Owner report recorded09:43 CET

After the scheduled first trial, the owner reported: “passed, 6w like last try”. This is owner-reported success and approximately6W for `path-d3-v2-20261101`; the independent review above establishes the recorded return and cleanup. The comparison is retained verbatim alongside the earlier3W report; it does not establish a measured power difference or exact duration. The dated09:13–09:40 graph remains missing.

## Historical first-slot preparation at08:52 CET

Trial **`path-d3-v2-20261101`** is armed for **09:15 November1 CET**, with Pi WoL09:31/09:31:30, independent RTC09:35, probe09:36:30 and frozen Pi snapshot09:38. The exact10:30 follow-up task was verified enabled and due immediately before arming. Entry and return have not yet been observed at this handback. The08:30 task deleted its exact ID before machine access; no additional workflow was scheduled.

The [preparation evidence](preparation/README.md) contains awake packet delivery, live topology/source analysis, preflight, tests/builds, manifest and timer receipts. The functional change is NIC`0000:03:00.1/d3cold_allowed` **1→0**, intended to keep its upstream bridge`0000:00:1c.3` inD0. Six original-off PCI/ACPI debug sites measure actual NIC/bridge suspend states and wake arming. This tests a firmware-owned PME power-path hypothesis; neither a disabled ancestor nor a faulty NIC has been established. Normal settings are restored only after safe return.

The10:30 review used `.scratch/pm-wake-path-20261101-0830/operator` with its verified `operator-v2-pins.sha256` and collected **once**, into `.scratch/pm-wake-path-20261101-0830/path-d3-v2-20261101`. Do not rerun that collector. Remote manifest `/var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf`; primary unit`predator-pm-path-path-d3-v2-20261101.service`. `path-d3-20261101` remains an earlier failed preflight with no power transition, not a second trial.

## New owner evidence and authorization

After the October31 review, the owner reported: “it went to 3w. schedule the next tests. make each test count.” This confirms an owner-observed3W low-power level for `wake-20261031`. It does not supply the dated22:13 to22:40 graph or exact time spent at that level. Keep the sealed [22:45 review](../s2idle-wake-validation-20261031/review-2245/README.md), its original full_passfalse result and checksums intact; this is a later addendum, not a rewritten original.

The trial returned on boot`f9b45aa6-b820-4ab5-b18c-9324d8d53087` around22:35, with counters1/0 and markers0 after successful cleanup. Both Pi UDP WoL sends ran without a recorded return then. Alarm readback/expiry, coherent pre-correction RTC/realtime, IRQ9 and ACPI fixed-event wake strongly support the RTC fallback. The exact fixed-event bit was not captured. Physical-duration precision still needs the graph; its absence does not prevent a distinct, justified next experiment.

The request authorized a bounded November1 sequence: at most two evidence-led s2idle trials, with independent RTC fallback and a review between trials. The October31 scheduling turn performed no machine access. Machine work is confined to November1,08:00 through23:00 Europe/Paris. The reviewed result and closed second slot are recorded above.

## Scheduled workflow slots

- **08:30 CET, November1:** inspect packet delivery and the NIC wake path, then prepare at most one justified single-variable s2idle WoL experiment. The09:15/09:31/09:31:30/09:35/09:36:30/09:38 schedule was armed at08:52. The preparation protocol required more300seconds lead and entry by10:00 so the result is available at10:30.
- **10:30 CET, November1:** independently review the first result, then prepare at most one distinct follow-up if the first returned and full health/cleanup pass. A second trial requires its own exact schedule and self-deleting read-only review, installed before arming. It must enter by13:00 and finish review by14:00. If the first is still executing, unreachable, unreviewed or blocked, the second does not run.

Exact task IDs, historical returned nextRunAt values and scope are retained in `schedule.json`; task prompts and creation receipts remain in this thread. Each task deleted only its own exact ID before machine action and applied its date/window guard. Both are now deleted. No recurring power-test loop is authorized.

The daytime preparation synchronizes only reviewed source/docs/evidence after fresh remote status, flake.lock and exact preimage checks; the guarded transfer receipt records its result. Historical review archives remain sealed.

## What each trial must distinguish

Begin with read-only evidence. Verify actual NIC identity/MAC/interface and PCI ancestry rather than assuming cached addresses; the previous baseline used`enp3s0f1`, NIC`0000:03:00.1` and parent`0000:00:1c.3`. Collect WoLg, endpoint/ancestor wakeup settings, ACPI wake mapping, readable PME/PMCSR and power/runtime/D3cold state, r8169/PHY details, interrupts and ACPI/wakeup-source counters. Do not use raw register writes. Read matching6.18.58 driver/PCI/ACPI source to distinguish required wake routing from merely suggestive settings.

Before suspending, capture and decode a bounded, explicitly labelled magic packet on Predator while it is awake. Validate the real ingress interface, destination MAC, broadcast/transport and payload, with a corresponding Pi sender capture when available. This checks the packet path without spending a suspend cycle. Do not send such a packet if the host's awake state is uncertain. Sender exit0 alone does not prove receipt by the sleeping NIC.

Choose one supported hypothesis after that inspection. A disabled necessary ancestor wake setting is a candidate only if the topology/source establish its relevance. Capture exact original values, apply only the chosen reversible setting, verify readback and restore it after safe return. If settings already agree, do not blindly toggle ACPI wake, disable ASPM, unbind drivers or rerun the same baseline. A packet-transport experiment or another single wake-path control requires a written reason explaining how either outcome changes the diagnosis. With no discriminating intervention or new measurement, leave Predator awake and report the specific blocker.

Record hypothesis, changed variable, baseline, predicted outcomes, data to collect and rollback in the new manifest before arming. Use separate state/schema/runtime pins for the November1 experiment and truthful cumulative history. Existing runners/manifests are historical; do not bypass their October31/fresh-boot guards or overwrite their source/state. Pin and test any required new Rust code and Nix package before use. Operational scripts belong in Rust; small shell adapters may only collect data or perform exact guarded transfers as before.

The first trial compares the supported wake-path change with the established3W/RTC-return baseline. The10:30 review decides the second:

- If the change produces a verified WoL-window return with healthy cleanup, test whether that result survives normal `pm_async=1`, keeping the successful wake intervention and all other controlled settings the same. This tests applicability to ordinary asynchronous suspension, rather than merely repeating the serial result. Record and reapply the reviewed intervention explicitly if the first cleanup restored its original setting.
- If WoL still fails but RTC returns and health passes, select a different, evidence-supported wake-path factor. Restore the first factor before changing the next. Do not combine speculative changes.
- A failure to return, uncertain cleanup or changed boot means evidence collection and owner notification only. No second sleep, recovery wake or automatic reset.

The50.450394second `intel_pch_thermal` noirq callback is a measured entry delay, not an established WoL/S3 culprit. Preserve temperature/callback evidence in the read-only capture; this sequence does not disable thermal protections. Earlier15W S3 failures are already established. This sequence does not repeat deepS3 or the diagnostic ladder, reboot to vary age, or change firmware/kernel/security/storage settings.

## Common execution and review requirements

Use the frozen system/kernel/modules, recorderN, trace0, pm_testnone, normal synchronized UTC RTC/NTP, the existing tested clock helper, and the serial baseline unless the second trial explicitly tests async1. Retain all GCroots and historical originals. Recheck current boot/btime/history, inactive systemd-suspend/MainPID0/no queued sleep, no HOLD or conflicting maintenance/timers, all five authenticated boot/late-rejection audits, TPMCRB/Acer/rawEC exclusions, AC/lid/framebuffer, LAN/DNS/NTP, pools/five mounts and application health. An unexpected boot or history change is a blocker, not permission to silently repin. Observe current Nextcloud cron cadence and avoid it.

Require synchronized independent Pi time, a live netconsole receiver, a fresh nonce reaching the capture, pinned waker, frozen baseline and bounded observer jobs. Announce exact entry/WoL/retry/RTC/probe/snapshot/review times and request the corresponding dated power graph before arming with more300seconds lead. Retain the proven relative timing: WoL due+960, retry+990, independent RTC due+1200, probe RTC+90, snapshot RTC+180. Permit only those scheduled wake sends. Validate the owned absolute RTC alarm by write/readback before requesting suspend; a missing or failed fallback prevents entry. Refuse a foreign alarm.

Capture fixed-event/IRQ/RTC/PME/wakeup-source baselines before entry and raw return before corrections. Keep kernel source monotonic, source boottime, journal receipt monotonic and receipt realtime separate. Genuine s2idle timekeeping suspension is valid. Classify wake windows separately from causes; neither receipt deltas, scheduled delays nor source-loop duration alone establish physical sleep. Preserve the original artifacts, actual Pi transmissions and graph/owner observations without upgrading uncertain evidence.

Cleanup must first prove systemd-suspend inactive/MainPID0 and no queued sleep job. Restore only owned settings/alarms; never kill an active sleep runner or remove its wake coverage. A hung/unreachable host receives bounded evidence collection and an owner report, no extra WoL, watchdog, crash, reset or retry. Do not restore twice after successful cleanup.

After independent collection and health checks, archive/checksum the new evidence and synchronize only reviewed paths after fresh remote Git status, flake.lock inspection and exact destination preimages. No commits/pushes, whole-tree sync, system activation, delegates or changes to unrelated work. At most two cycles total in this sequence; after the second, review and hand back rather than creating another power-test chain.
