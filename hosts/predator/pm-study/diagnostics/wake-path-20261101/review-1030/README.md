# November 1, 10:30 wake-path review

`path-d3-v2-20261101` completed one genuine s2idle cycle on the expected boot and returned in the **09:35 RTC-fallback window**, not either scheduled WoL window. The NIC D3cold restriction produced the intended D0 upstream bridge, D3hot NIC and kernel-reported native PME/ACPI wake arming, but was insufficient to make WoL return the system. Cleanup passed. No second trial was prepared or armed.

The owner reported “passed, 6w like last try” at09:43. Preserve this as observed low power of approximately6W and owner-reported success. It does not identify the wake cause or establish an exact duration or measured difference from the earlier3W report. The dated09:13–09:40 Europe/Paris power graph remains missing; original `full_pass=false` and physical duration unknown remain unchanged.

## Independent verification

The exact10:30 follow-up task deleted itself before machine access. Date/window guard passed10:30:29CET. The pinned original operator collected once; its scripts, status and authenticated results are in `operator-collection/`. Independent retrieval obtained24 originals and both frozen Pi files. `originals.tar.xz` and `original-metadata.txt` preserve source root0600 metadata; extracted copies belong to the local user. All24 SHA-256 values match the authenticated collection and fresh metadata capture. Manifest SHA-256 is `ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653`.

The offline Rust verifier in `verifier/` imports no operational runtime. Its pinned serde_json1.0.151 run passed: `independent-verification.json`. It verifies one new cycle, cumulative success1→2/fail0/markers0, the unchanged October31 cycle, and preservation of all2102 before-kernel records by journal cursor. All records use boot `f9b45aa6-b820-4ab5-b18c-9324d8d53087`; btime remains1793474520. Request, operation, evidence and cleanup succeeded. Both primary and systemd-suspend were inactive/MainPID0/Resultsuccess/ExecMainStatus0, with no jobs, at10:33.

The immutable November1 preparation, historical runtime sources and sealed October31 review passed their original checksums. The earlier `path-d3-20261101` failed preflight remains a zero-transition preparation, not another sleep cycle. Only one of the two permitted November1 transitions was used.

## Actual wake sequence

- 09:15:11: RTC and realtime1793520911; owned alarm read back as1793522100 (09:35), alarm IRQ enabled. Recorder off, markers0, Timex8193 with STA_UNSYNC clear.
- 09:31:00.650861 and09:31:30.641785: Pi canonical WoL sender receipts; both services completed successfully. No return was recorded at either time. Sender success does not prove sleeping-NIC reception.
- 09:35:03.594471: systemd-suspend return receipt; finish09:35:03.606099; logind finish09:35:03.623327. Logind contains the trial request and completion, with no lid/key event.
- 09:35:04: original pre-helper return RTC/realtime1793522104 agree; alarm empty/IRQ disabled, wakeIRQ9, recorder off/markers0, Timex8193. Kernel reports `ACPI: PM: ACPI fixed event wakeup`.
- 09:35:07.441572: fresh NTP; restored RTC08:35:08.504276UTC. Original clocks were already coherent before correction. Completion09:35:24 records `RTC_FALLBACK_WINDOW_RETURN`.
- 09:36:30.601297: Pi SSH banner; host key09:36:30.807938, probe success09:36:30.809588. Frozen snapshot completed09:38:00.571598. No current-trial timers remain.

Pi baseline SHA-256 `7a048b004674d029f7db85b196fedd4df07821bdeaab81d2863044143c0c72c2`; result `1f9fd1adc4703040d09ec15f05561ac14e8174de9c8443d521609926a51825b8`. Independent copies match; the baseline is an exact prefix of the result, with one new trial BEGIN and RETURN and the expected nonce. Netconsole silence between NIC suspend and return is a transport limit, not a physical-duration measurement.

RTC fallback is strongly supported by the expired alarm, coherent raw clocks, IRQ9/fixed-event message and RTC counters. Armed→raw `ff_rt_clk`20→22; power/sleep fixed counters remained0. First RTC wakeup-source row active23→25/events43→47, wakeup_count0 unchanged; NIC/bridge/lid/power rows did not change. RTC accesses/programming can contribute activity, and the exact fixed-event status bit was not captured at wake. GPE66 increased12→15 and SCI202→207; these do not uniquely identify a wake cause.

## What the experiment established

Between the new s2idle entry and idle loop, kernel-source timestamps show:

- 46393181949µs: `r8169 0000:03:00.1: PME# enabled`.
- 46393181989µs: `pcieport 0000:00:1c.3: Wakeup enabled by ACPI`.
- 46393194491µs: NIC suspend stateD3hot.
- 46393221824µs: upstream bridge suspend stateD0.

The original measurement result is `Ok(())`. Raw return still has owned NICd3cold0 and six debug sites enabled; restored evidence and the fresh10:33 check show NIC/sibling/parentd3cold1, six original-off flags, ordinarydeep/none/async1/trace0, recorderN and empty alarm. Cleanup was not rerun during review.

Root-port PCIe RootStatus changed **0x00030201→0x00030301**, requester0x0201→0x0301 (the actual03:00.1 NIC). PME status and pending were already set before entry and remain set; native PME interrupt enable was clear in both snapshots. Matching6.18.58 definitions and native-handler source are in `reference/`. This is a useful routing clue, but these untimed snapshots do not prove when the requester changed, sleeping-packet receipt, or an exact firmware failure. Firmware withheld native PME ownership at boot, so the disabled native interrupt is not by itself a defect.

## Firmware mapping and remaining blocker

A guarded read-only retrieval saved the DSDT and13 SSDTs in `acpi-tables.tar.xz`. Local iasl20251212 emitted a complete DSDT but the first all-table batch exited255 with duplicate-name diagnostics for later SSDTs; both logs are preserved. A separate DSDT-only disassembly with SSDT externals completed successfully, retaining unresolved-symbol/BNUM warnings. `dsdt-single.dsl` is static firmware evidence, not proof that any method executed during the trial.

The DSDT maps RP04 to00:1c.3 and its `_PRW` to GPE0x69. `_L69` calls RP04.HPME; when its chipset PMSX condition is true, HPME notifies PXSX and clears status. PXSX address0 is sibling card reader03:00.0, not NICfunction1. GLAN is a separate00:1f.6 device with GPE0x6D. `_L66` dispatches graphics GSCI, so the captured GPE66 increment cannot be treated as evidence of NIC PME delivery.

At10:39:20, the same boot still had cumulative GPE69 count0, disabled/unmasked while awake; GPE66 count16, GPE6D count0. This is not an in-sleep enable-mask snapshot. The trial captured GPE14/66, not GPE69 at the suspend boundary; this measurement gap remains explicit. Matching `drivers/acpi/device_pm.c` registers an ACPI wake source independently of the device's wake-enabled policy and handles notifications when its notifier is present; `drivers/pci/pci-acpi.c` registers that notifier for ACPI-backed PCI devices. Therefore the disabled sibling wake policy alone does not explain an absent notification, and blindly enabling it or unrelated GLAN is not justified. The unresolved boundary is firmware/chipset PME-to-GPE/notification handling, including whether the mapped GPE was armed and signaled during sleep. No raw register write, ACPI method invocation or settings change was used to investigate it.

## Four clock domains and entry latency

All entries below are microseconds. Source values and receipt values are distinct domains; they must not be subtracted across columns.

| Phase | Source monotonic | Source boottime | Receipt monotonic | Receipt realtime |
|---|---:|---:|---:|---:|
| s2idle entry |46391322191|46391322191|45255075516|1793520912182851|
| idle loop |46422106201|46422106201|45289594362|1793522103735087|
| loop resume |47579130966|47579130966|45289594488|1793522103735214|
| suspend exit |47582724228|47582724228|45289745593|1793522103886319|

Loop interval is **1157.024765s** in both source fields, versus126µs receipt-monotonic and127µs receipt-realtime. Kernel `Timekeeping suspended for 1157.033 seconds` is valid s2idle behavior. Buffered kernel receipts follow the systemd return/finish receipts. Neither source interval nor receipt interval establishes physical low-power duration.

Entry-to-loop source interval30.784010s includes successful `intel_pch_thermal` noirq callback28.719600s, total noirq28.964737s. This remains a measured delay, not a demonstrated WoL or S3 cause.

## Fresh health and second-slot decision

The10:31:57–10:32:06 health capture and10:33 final-state capture verified unchanged boot/system/kernel/modules; all five authenticated boot and late-phase rejection audits; TPM CRB/Acer/raw-EC exclusions; six HOLD absences; ordinary PM/RTC settings; AC1/lidclosed/fbblank4; LAN1000/full/WoLg/declaredUUID/DNS; synchronized UTC RTC/NTP; online pools with zero errors, five correct mounts, active application services and four healthy Immich containers. Fresh Pi10:33 retained its expected boot, active receiver/MainPID1974, no jobs, synchronized NTP and pinned waker.

**Maintenance preflight did not pass:** tank scrub was active since10:07:38,56.85% complete at the health capture, with `zfs-scrub.service` running. Fast scrub completed10:10:39 with zero errors. Nothing interrupted the scrub. Thus “healthy services and restored settings” must not be reported as a fully passing new-suspend preflight.

The second slot is **NOT_ARMED**. The RTC-only result does not satisfy the verified-WoL prerequisite for the normalasync1 branch. The D0 bridge measurement rules out that tested restriction as sufficient, while the remaining firmware-routing gap does not establish another reversible setting to change. A speculative sibling/GLAN toggle or unchanged repeat would not meet the protocol. No new runtime, manifest, machine timer or read-only reviewer was needed; no further power workflow was scheduled. Predator remains awake with the original settings restored.

The review archive and five status/index files are the only intended synchronization paths. Transfer receipts live separately in `.scratch/pm-wake-path-review-sync-20261101-1030/`; they record fresh remote status, lock/preimages, exact checksums and installation result. Preparation and October31 sealed archives remain unchanged.
