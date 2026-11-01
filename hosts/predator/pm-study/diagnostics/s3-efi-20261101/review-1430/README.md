# RTC-only S3 return reviewed, November 1, 14:30 CET

**The real-S3 attempt returned on the same boot.** The original runtime completed successfully, all three EFI boundary writes succeeded, and cleanup and fresh health passed. The hang was not reproduced in this trial. Return timing and the expired RTC alarm support RTC wake, but the exact wake event and any owner interaction remain unverified. The dated14:18–14:30 power trace is still missing, so no physical low-power duration or full physical PASS is claimed.

This was a read-only review. The exact14:30 task deleted itself before machine actions. The pinned collector ran once; original files were then retrieved independently. No cleanup was repeated and no further power workflow was scheduled.

## Identity and independent verification

Trial`s3-efi-v2-20261101`, nonce`deep1101a`, boot`39d7579b-b86d-4d30-b910-131943e550c8`, reviewed and observed btime1793535079. ManifestSHA256`bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c`; root state`/var/lib/predator-pm-efi-s3-reviewed`. The runtime is the pinned2bis3s5m9djibh5fjgrjknpwah6w60rv package, binarySHA`810564d7fc6d6ab3331220ce07e9c9a9959debde3d8299179c106881bef23e3b`; stage moduleSHA`90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0`.

The standalone offline Rust verifier imports no operational runtime. It verified all21 trial originals against the authenticated collection, their source root0600 metadata, the separate root0600 core-review anchor SHA`78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896`, and all2122 preceding kernel records by exact journal cursor/content. The tar retains source ownership/modes; extracted local copies have local ownership. The preceding core cycle is preserved exactly. There is one new deep entry/exit, one `ACPI: PM: Low-level resume complete`, no new diagnostic checkpoint, no s2idle and no BUG/Oops/Call Trace. Cumulative counters1→2, fail0, old RTC recorder markers0.

The original `completed.json` reports`S3_LOWLEVEL_RETURN`, successBefore1/successAfter2, restorationPASS and completedEpoch1793539524. Original request, operation, evidence and cleanup all succeeded. Unlike the prior core harness, this trial has its own successful completion, verification and after-kernel originals. The earlier core's failed exact-btime check and absent completion remain unchanged.

## Actual sequence and wake evidence

-14:20:10.279642: runtime BEGIN. Armed snapshot epoch/RTC1793539210, absolute alarm1793539500, `/proc/driver/rtc` alarm13:25:00UTC on November1 and alarm_IRQyes. Async0, pm_testnone, normal RTC/NTP and trace0.
-14:20:11.072332: logind recorded the request from the trial's busctl; suspend entry receipt14:20:11.225475.
-14:25:03.934847: systemd reported return; finished14:25:03.941716; logind finished14:25:03.954525. Buffered kernel records arrived afterwards and retain their separate source timestamps.
-14:25:04: raw pre-correction snapshot epoch1793539504, RTCepoch1793539505, hwclock13:25:05.010877UTC. Alarm empty, alarm_IRQno, Timex8193 with STA_UNSYNC clear. Clocks were already coherent before the helper.
-Fresh NTP handoff synchronized to14:25:08.180379. Completion and runtime return marker followed at14:25:24, with unchanged boot and btime.

There was no WoL in this experiment. RTC timing, alarm expiry and the RTC fixed-event counter12→13 support RTC wake. They do not prove the exact latched wake bit. `/sys/power/pm_wakeup_irq` returned ENODATA both before and after; no IRQ9 or generic ACPI fixed-event wake message is claimed for this S3 trial.

Other armed-to-raw counters: fixed power/sleep0→0; GPE14 47→47, GPE66 7→9, GPE69 0→0, GPE6D 0→0; SCI66→69, SCI_not1→1, GPEall54→56, error0. RTC wakeup-source instance0 active14→16/events26→29; two PNP0C0C power-button source instances also rose2→4 and1→2. Their wakeup_count remained0; lid/NIC rows were unchanged. No lid or power-key event appeared in the bounded logind journal. These observations do not establish whether a person touched a button; that confirmation was requested. RTC reads/programming and suspend/resume handling can affect counters, so a counter increase alone is not a unique trigger proof.

Root-port PCIe RootStatus was0 before and after; complete256-byte root-port/card-reader/NIC snapshots are retained. These are boundary/post-return registers, not in-sleep capture. GPE69 remained0 in this no-WoL test, which does not resolve the separate firmware PME/WoL path.

## EFI boundary record and capacity

The ordered kernel records show phase20 before the low-level ACPI call, phase21 after its return, and phase22 after syscore resume. All have sequence1/2/3, context7, error0 and EFIstatus0. The final direct firmware readback has attributes7 and exactly64 bytes: `PMS3BND2`, version2/length64, phase22, sequence3, flags7, this boot UUID, nonce`deep1101a`, zero padding/reserved fields and independently verified IEEE CRC32`0x210ce3de`. Flags7 mean IRQs disabled, SYSTEM_SUSPEND and one CPU online.

The module reports attempted3/completed3, disarmed, error0, results`0,0,0`, EFIstatuses`0,0,0`. Reported free space7034→6572, cost462 bytes, total163752 unchanged, maximum6974→6512. This stays within3×256 bytes and preserves Linux's5120-byte reserve plus1024 margin. Current usable space above the reserve is1452 bytes. The same three-write admission threshold is6912, greater than remaining6572; this review neither deletes records nor authorizes another sequence.

Fast-clock deltas were0/0/45854846ns. The first two clocks were frozen; these values do not measure the suspended interval. The final S3 record has not itself been tested through another reboot or power loss. The earlier awake record's planned-reboot retention proof remains separate.

## Four timestamp domains

All values below are microseconds; source and journal receipt clocks are never substituted for one another.

| Event | Source monotonic | Source boottime | Receipt monotonic | Receipt realtime |
| --- | ---: | ---: | ---: | ---: |
| deep entry |4132609255|4132609255|4127559046|1793539211225475|
| phase20 begin |4134399610|4134399610|4133381114|1793539504033839|
| phase20 end |4134447713|4134447713|4133381139|1793539504033864|
| low-level resume complete |4134449257|4134449257|4133381163|1793539504033888|
| phase21 begin |4134449271|4134449271|4133381196|1793539504033921|
| phase21 end |4134494786|4134494786|4133381221|1793539504033946|
| phase22 begin |4134495713|4134495713|4133381686|1793539504034411|
| phase22 end |4134541574|4134541574|4133381710|1793539504034435|
| suspend exit |4138429965|4138429965|4133474997|1793539504127723|

Entry-to-exit deltas are5.820710s in both source fields,5.915951s receipt-monotonic and292.902248s receipt-realtime. The kernel reported timekeeping suspended286.986seconds. These quantities measure different clocks and are not a substitute for the physical power trace. The frozen source clock particularly makes the low-level-call source delta unsuitable as sleep duration.

The successful intel_pch_thermal noirq callback took11910µs; total noirq suspend339.703ms. The long thermal callback seen in prior s2idle trials did not occur here. That observation alone establishes neither a cause nor a fix for the historical hang.

## Pi evidence and restored health

Independently retrieved frozen baselineSHA`1c6b69b7ca5f33d7a31d400ac42acaf365403dcbbf103952de1999e0679588e5`, resultSHA`ca27a5afad8035b7a0f38eeac245393d3fe665c126df8e0401970eb3d58402ef`. The baseline is an exact byte prefix; suffix has one trial BEGIN and one successful RETURN. Netconsole ends at the NIC suspend callback around14:20:12, then records link-up14:25:08 and RETURN14:25:24. Silence while the NIC was unavailable does not establish low-power duration.

Actual Pi probe: SSH banner14:26:30.636663, host key14:26:30.866014, service completed14:26:30.869447. Frozen snapshot completed14:28:00.578920. No current trial timers remain. Fresh Pi proof14:34:56 retained boot2142644e-84db-40ae-b1b9-9e8c71265cf7, receiveractive/MainPID1974, no jobs, synchronized stratum2 NTP, offset+431µs/delay9.662ms/jitter1.128ms. Historical notice-start timers have no next activation.

Fresh Predator health14:31:32–40 passed: same current/booted system`zbq64a1n4yzr3knpsgzhiw29lvgs3b34`, kernel`82haph8j8hmbny68rzkis8dqrz9gqrr5` Linux6.18.58, modules`1qbmxim06wak3qplmj747hb5gkyzgj9z`, boot and btime; running/no jobs/failed units. Both sleep and primary are inactive/MainPID0/Resultsuccess/ExecMainStatus0. Stage/compact/probe modules absent; async1/none/deep/trace0/RTCrecorderN/markers0/emptyalarm. NIC/card-reader/root-port d3cold_allowed1, AC1/lidclosed/fbblank4.

All five authenticated boot and late-phase rejection audits passed, as did TPM CRB/Acer/raw-EC and nine HOLD checks. LAN1000/full/WoLg/declared UUID/DNS, normal UTC/RTC/NTP, both poolsONLINE/no errors, five dataset mounts, application services and four healthy Immich containers passed. Scrubs were complete; no active maintenance/builders. NTP172.234.184.36 had offset+2.375ms/delay4.209ms/jitter1.602ms. Cron last fired14:31:19, next due14:36:19. Night/next suspend timers remain inactive. Original runtime/module/core/historical hashes were rechecked, and all GCroots were preserved.

## Conclusion and remaining boundary

This demonstrates a successful normal-clock, serial, RTC-backed real-S3 software return with the frozen kernel/security/device baseline and this instrumentation. The observed path crossed low-level resume, syscore resume and later device/service recovery. It does not identify the cause of earlier15W hangs, prove repeatability, establish WoL operation, or isolate recorder timing, wake-source dependence or intermittency. The missing dated graph and owner-interaction confirmation remain necessary for physical interpretation.

The review is closed with no additional test scheduled. Preparation/core/older archives stay sealed; current results and status updates are synchronized only through fresh Git/flake-lock/preimage guards.
