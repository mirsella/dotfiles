# Normal-clock S3 boundary investigation, November 1

## Subsequent meter clarification and scheduled control

The owner confirms constant~27W for the full discussed interval, measured across **laptop and peripherals**, and further clarifies that this has always been the identical load: approximately3W proper suspend,15W hang,27W normal-on. That makes the same-meter comparison useful without assigning watts to individual components. The [recorder-off control](../s3-recorder-off-20261101/preparation/README.md) is now armed for16:15 entry/16:20 RTC and16:30 read-only review, with owner recovery availability confirmed. It removes only the EFI boundary recorder and makes zero protocol EFI writes; actual result is pending.

## Historical owner power observation recorded14:49 CET

The owner initially reported [approximately27W](owner-power-1449/README.md) and doubted that the machine slept. **The verified result is a software suspend/resume return, not a demonstrated low-power S3 success.** The dated14:49 addendum preceded confirmation of a sustained reading and disclosure of the combined meter boundary; use the clarification above for the current interpretation. The earlier “S3 resumed successfully” wording was too broad.

All original software/recording/cleanup evidence remains valid and sealed. It does not distinguish failed platform power entry from components staying powered. The unchanged14:49 addendum is included in the subsequent guarded recorder-off preparation synchronization; its own turn performed no machine or power action.

## Historical software review at 14:30 CET

The [RTC-only S3 software path returned](review-1430/README.md) on the same boot, with all three EFI boundary writes, cleanup and fresh14:31 health independently verified. Cumulative1→2/fail0/oldRTCmarkers0. The original runtime completed successfully; all21 originals and the separate core anchor are preserved. Return14:25, alarm expiry and RTC counter activity support RTC wake, but no wake IRQ was recorded and owner interaction remains unconfirmed. The dated14:18–14:30 power trace is still required for physical interpretation. There was no non-returning software stall; the later owner report raises a high-power failure. No further power test is scheduled.

The14:30 reviewer deleted its exact task and collected once. Do not repeat collection or completed cleanup. The preparation and core review below retain their historical scope and sealed checksums.

## Historical preparation at 14:06 CET

The [core control was independently verified](core-review/README.md): both EFI writes and cleanup succeeded on the same boot. Its original harness subsequently failed an exact wall-clock btime estimate after a one-second NTP change; that failure and absent original completion remain preserved. Full13:54 health passed. The14:00 fallback review was deleted after main-thread review; the once-only core collector has already run.

One [distinct RTC-only real-S3 attempt](s3-preparation/README.md) is now armed: entry14:20, RTC14:25, Pi probe14:26:30, frozen log14:28, read-only review14:30. Trial`s3-efi-v2-20261101`, nonce`deep1101a`; new reviewed state and pinned independent core anchor preserve truthful history. No WoL or automatic retry. The owner received exact times and a14:18–14:30 power-trace request before arming.

## Historical core preparation at 13:35 CET

The owner requested suspend-hang testing with redundant Pi logging and confirmed local recovery and a timestamped power trace were available. One separately announced ordinary reboot verified retention of the compact EFI record. One recorder control is now armed for **13:45 CET**; actual S3 is still conditional on its independent review.

The control is `core-efi-20261101`, nonce `core1101a`, root manifest `/var/lib/predator-pm-efi-s3/core-efi-20261101-plan.conf`, SHA256 `95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3`. Its service is `predator-pm-boundary-core-efi-20261101.service`. Pi reachability probe is13:48, frozen netconsole snapshot13:49, and a self-deleting read-only fallback reviewer is scheduled14:00. No WoL is scheduled. The owner received these times and the13:43–13:50 power-graph request before arming at13:34:32.

This is a new bounded protocol following the earlier closed wake-path investigation. Older dated/fresh-boot runtime guards and sealed evidence remain intact. All operations are November1 Europe/Paris08:00–23:00. The scope is one recorder control followed by at most one separately announced RTC-only S3 attempt if the control, capacity audit, restoration and full health pass. No automatic power retry or further chain is authorized by this file.

## Retention proof and current boot

The ordinary reboot was requested13:10:53.607621141, epoch1793535053, from boot`bf51c3ac-164e-4bfa-b56c-7b71df526b7f`. The same ordinary signed generation167 UKI was revalidated as default and selected, SHA256`63a2619ff831156ba51c08c537efafa47a5c4c1e07ff5c52a7253435c7acdcd3`. New boot is **`39d7579b-b86d-4d30-b910-131943e550c8`**, btime**1793535078**, about13:11:18. No new EFI write was made for retention testing.

At13:14:45 the efivarfs file `PMS3-20261101-a-47d95147-3665-4bda-8ddf-d48f0e339023` contained exactly the original four-byte attributes7 plus64-byte record. SHA256`176cbbc62c46cd68c977a07d6c3f775d770ff0983176190646c17f9c18d37b63`; original boot/nonce/phase1/sequence1/checksum`0x5c258a74` were unchanged. The comparison used the independently verified original readback. This proves retention through this planned normal reboot, not arbitrary power-loss persistence.

Current and booted system remain `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`, kernel `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`, modules `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`. The frozen clock handoff succeeded naturally. NVRAM free7342/usable2222 bytes, unchanged by reboot. Fresh counters0/0; the older boots' histories are preserved rather than reinterpreted as this boot's baseline.

## Why this control counts

The old RTC phase recorder changed timekeeping. The compact record has now passed an awake write/readback and reboot retention, but EFI calls with syscore suspended have not been proven. Matching6.18.58 `drivers/base/syscore.c` places `syscore_suspend` end after its callbacks and `syscore_resume` start before resume callbacks, with interrupts disabled and one CPU online. `pm_test=core` exercises these points and skips actual platform entry.

The new small C adapter, `pm_boundary_stage`, registers only the existing `suspend_resume` tracepoint. Operational policy, manifests, checks, collection and verification are Rust. It uses the same private GUID and64-byte `PMS3BND2` format with trial nonce, boot UUID, phase, sequence, context flags and CRC32. It never writes the RTC, allocates firmware garbage-collection probes, deletes variables or changes kernel/firmware/security settings.

Control records are phase10 after syscore suspend, then phase11 before syscore resume, sequences1/2. Both require flags7: IRQs disabled, `SYSTEM_SUSPEND`, one online CPU. Only two ordered attempts are permitted; error or unexpected order disarms the adapter. The proposed S3 mode permits exactly three ordered records: phase20 immediately before ACPI's low-level S3 call, phase21 after it returns, phase22 after syscore resume. These locations are not claims of physical sleep, and a missing record may also reflect a failed recorder write.

Each callback uses EFIVAR trylocks and nonblocking reserve checking/writes. The kernel's5120-byte reserve remains enabled, with an extra1024-byte margin and a256-byte budget per attempted write. The module requires capacity for the whole sequence before loading; post-return verification checks the measured total storage decrease and remaining margin. It retains dynamic maximum-variable-size semantics established by the earlier probe. Fast-clock differences are diagnostic values, not trustworthy elapsed durations while timekeeping is suspended.

Core control changes only the new recorder plus temporary serial `pm_async=0` and `pm_test=core`; ordinary deep is retained. Normal synchronized UTC RTC/NTP, trace0 and the old RTC recorder off are required. A successful control should have one diagnostic core checkpoint and no low-level S3 return, cumulative0→1, both EFI writes successful, exact final64-byte readback, safe storage budget, normal clocks and complete restoration. A failed or stalled control blocks S3 rather than being relabelled as the original S3 hang.

## Pins and checks

- Runtime `/nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial`, SHA256`12b551f868bb0e0eae8c012bc9e41a0fbbc6e4dc975bf651e3a538e1651ed886`.
- Runtime source SHA256`4884bb9d16b073785e441a17f807a6ce130f53e6b43a0a0cbdb9e41cd0a19f7f`; pure record/timing source`8699c8457d330df8341b529e13cdaf437e50fd1035c576cbd98b4d8e89cdc2e7`.
- Module package `/nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101`, module SHA256`90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0`, C source`aeeba42b27ef3c2448626e9557bb14494190b51833c63e78c81dba8610a537e7`.
- Both new remote GCroots and older roots remain. The Nix attribute builds the module separately; nothing is autoloaded or activated into a new system.
- Six focused tests passed: record/CRC/context identity, bounded complete write sequence, four separate journal clocks and diagnostic/real-cycle distinction, inactive-sleep cleanup, timex ABI, and strict manifest/history/timing. Module/runtime builds and final flake evaluation passed. Initial syntax error and missing Git-index inclusion of unchanged historical sources are retained in build logs; corrected builds are the pinned artifacts.
- Full runtime checks passed before staging and again before arming: frozen identities, five authenticated boot/late-phase rejection audits, TPM CRB/Acer/raw-EC exclusions, all HOLD checks, exact sleep configuration/unit, counters0/0, ordinary PM/RTC state, LAN/DNS/NTP, healthy pools/mounts/services/four containers, no jobs/maintenance/builders. Old hashes are pinned. Actual execution repeats these checks.
- Pi boot`2142644e-84db-40ae-b1b9-9e8c71265cf7`, synchronized NTP stratum2, receiver active, matching new nonce. BaselineSHA256`5c4218f7de7088a87b9fdc354e1e552d7c33e6313d9209f093542ac8e2055188` was independently retrieved and verified. Pi UDP netconsole is redundant with local journal and EFI storage, but cannot capture packets after the NIC suspends.

## Execution, recovery and interpretation

State is `/var/lib/predator-pm-efi-s3`, with new root0600 create-only artifacts. Primary service is Typeexec/Restartno/RuntimeMax900s/TimeoutStop180s, with pinned `ExecStopPost restore`. Restoration first requires inactive `systemd-suspend`, MainPID0 and no queued sleep. It captures raw return and the owned recorder status/readback, disarms/unloads only the matching module, restores none/async1, and uses the existing tested clock helper. Completed restoration is never repeated. No executing sleep is killed; on uncertainty, collect bounded evidence and notify the available owner.

All four journal domains remain separate: kernel source monotonic, source boottime, receipt monotonic, receipt realtime. Genuine timekeeping suspension is allowed. Neither buffered receipt intervals, source intervals nor an RTC schedule establish physical low-power duration. Snapshots capture fixed-event/IRQ/GPE69 and other wake counters, wakeup sources and PCI configuration before and after; post-return registers do not establish their exact in-sleep timing.

The now-armed distinct experiment changes `pm_test=core` to real S3 while retaining serial suspension, the tested recorder and ordinary device settings. It uses a separately written/read-back absolute RTC alarm at entry+300seconds, no WoL, owner power trace and local recovery. Its plan pins this boot's independent core review, requires cumulative1/0 and a fresh capacity/health preflight, and has a dated read-only review installed before arming. See s3-preparation for exact times and pins; the original failed core harness has no manufactured completion.

Interpret future evidence with the graph: no sustained low power narrows final entry/platform transition; low power without a recorded low-level return after the alarm leaves wake delivery/earliest resume unresolved; phase21 without phase22 narrows syscore return; phase22 before a stall moves investigation later into resume. A successful RTC-backed S3 return would justify investigating wake-source dependence or intermittency. An alternative kernel or component isolation should follow the observed boundary, not a speculative bundle.

Historical core trial `.scratch/pm-s3-rtc-20261101/core-efi-20261101` has already been collected exactly once and reviewed in core-review. Do not rerun its collector or completed cleanup. Current S3 trial and its separate read-only review are detailed in s3-preparation. Retrieval failures are not evidence of missing artifacts.
