# Recorder-off S3 control prepared November1,15:55 CET

One attempt is armed; execution and physical outcome have not yet been observed. Owner availability is16:15–16:30. Exact announcement requested a dated16:13–16:30 trace and any button/lid action before arming.

| Event | CET | Epoch |
| --- | --- | --- |
| Entry |16:15|1793546100|
| Owned RTC |16:20|1793546400|
| Pi probe |16:21:30|1793546490|
| Pi frozen log |16:23|1793546580|
| Read-only review |16:30|1793547000|

## Measurement and single variable

Owner: “same combined load. always the same thing. its always been the same thing. 3w correct suspend, 15w hang, 27w normal power on”. Same laptop-plus-peripherals meter/connections are confirmed. The reference values are owner observations, not controlled component measurements. The earlier sustained27W software-return trial did not reach the owner's normal low-power level. Keep all attached loads unchanged.

Hypothesis: late EFI SetVariable callbacks may perturb firmware power handling. Remove only the boundary recorder; keep serialasync0/pm_testnone/deep/trace0/oldRTCrecorderN, RTC-onlywake, normal synchronized clocks, signed system/kernel/modules/security/storage/device configuration and Pi logging. A lower same-load draw supports instrumentation sensitivity but not unique causality; unchanged high draw shows removing this recorder was insufficient. A hang without new breadcrumbs does not localize a specific boundary. No new EFI record should exist for this control.

The new Rust runtime has no module load/unload or EFI-writing path. It requires all three boundary/probe modules absent before request, raw snapshots and cleanup, and rejects their kernel activity in the new cycle. Its only operational mutations are owned serialasync/ordinarypm_test and the absolute RTC alarm, followed by the existing normal-clock handoff. Historical EFI files are read-only identity anchors.

## Read-only topology and health

Authenticated topology15:35:54–55 and fullhealth15:35:54–15:36:02 passed unchanged boot39d7579b-b86d-4d30-b910-131943e550c8/btime1793535079, cumulative2/0/RTCmarkers0, frozen systemzbq64a1n4yzr3knpsgzhiw29lvgs3b34, kernel82haph8j8hmbny68rzkis8dqrz9gqrr5 Linux6.18.58 and modules1qbmxim06wak3qplmj747hb5gkyzgj9z. Runtime checks repeated at15:54 and15:55, including exact prior core/S3 kernel-record preservation, original hashes, owner/meter manifest, all five authenticated boot/late-rejection audits, TPM CRB/Acer/raw-EC/HOLD exclusions, ordinary PM/RTC, sleep inactive/MainPID0/unqueued, network/clocks, pools/five mounts/apps/four healthy containers and no active maintenance/builders/conflicting timers. Actual cron cadence was recorded. Checks run again at execution.

Root is a128GB SATA SKhynix SSD; fast is a240GB SATA Crucial SSD. Tank's three USB HDDs are Toshiba750GB, Seagate1TB and WD1TB through a Genesys hub, with two Ugreen bridges. All storage connections/power policies stayed unchanged. USB descriptors are capabilities, not power measurements. AC online; battery full100%, current_now0. NVIDIA01:00.0 is unbound/D0 while awake, while its audio sibling is runtime-suspended; this does not identify its in-sleep state or justify changing it in this test. Full raw topology includes all USB/PCI devices and power attributes.

Fast/tank ONLINE, zero errors; scrubs completed10:10:39/10:52:07. Five mounts and all checked application units/containers healthy. LAN1000/full/WoLg, declared UUIDc4e6863b-7f76-4a4d-9a48-9937af0b5a36/DNS192.168.1.1; normal UTC RTC/NTP. OldRTCrecorderN/markers0, async1/none/deep/trace0 and emptyalarm at baseline. EFI usable1452/free6572 is preserved; no capacity guard was lowered and no variable was erased/reused.

## Immutable protocol and history

- Trial `off-s3-20261101`, nonce `off1101a`, state `/var/lib/predator-pm-recorder-off`.
- Schema `recorder-off-s3-20261101-1`; manifest SHA256 `9cbcb49aa9c5c61650874bbe8544af5b8241495f48368f4865c91ded9e957ba1`.
- Runtime `/nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0/bin/predator-pm-recorder-off`, SHA256 `94b44bc3d6c5a4666372cee0cb47176966b2d684060bd26145fe856975d5d377`.
- Source `recorder_off.rs`, SHA256 `a7285638fc2cef87ff86e5cec7a617093265f075e6dd216c7a7923bad1d7c270`.
- Existing qzs clock helper SHA256 `b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b`.

The manifest fixes recorderabsence/zeroEFIwrites, same-load measurement, hypothesis/prediction/rollback, owner window, prior core review and prior S3 completion. Prior core's post-cleanup btime guard failure and absent completion remain unchanged. Exact boot UUID and reviewed btime estimate±2s are checked separately, with drift logged. Two prior cycles, one checkpoint and one low-levelreturn must match original cursor/content evidence; expected successful cumulative count becomes3/0. No old runtime or source was edited to bypass its guards.

Eleven focused tests passed, including strict manifest/window/history, all prior record preservation, recorder activity refusal, foreign alarm/unsafe cleanup rejection, source-clock requirements, honest core-review status and CRC/shared verifier checks. Nix build passed. First flake evaluation timed out atpackages.ci with its interruption retained; second foreground `nix flake check . --no-build --no-update-lock-file` passed15:53. Runtime/source/operator pins and unchanged historical sources rechecked before arming. The new local and remote GCroots are retained; no system activation.

## Independent observer and arming

Pi expectedboot2142644e-84db-40ae-b1b9-9e8c71265cf7, active netconsole receiver, synchronized stratum2 NTP195.154.185.215;15:54 proof offset+1.125ms/delay9.567ms/jitter1.208ms/count645. Exact fresh READY nonce reached Pi. Independently retrieved baseline SHA256 `9896ebc4ee83074e3af6cdf5301f3532df0e23703967fb31999e8d215a201a40`. Canonical waker hash was recorded as provenance only; no WoL is scheduled or executed.

Known16:30 review task was independently verified enabled/due/current-thread at1793544903 before arming; exact receipt is in trial/review-schedule.json. Primary service `predator-pm-recorder-off-off-s3-20261101.service`: Typeexec,Restartno,RuntimeMax900s,TimeoutStop180s,pinned ExecStopPost. Pi timers armed15:55:34; primary15:55:35, inactive/MainPID0/notrequested at that point, over19minutes lead. Observers are bounded one-shots. The absolute owned RTC alarm is written/read back with IRQ enabled immediately before the suspend request, not claimed already programmed at preparation time.

## Review and ownership

Pinned controller and trial files are in `.scratch/pm-recorder-off-20261101/`. Collect once with `operator collect .scratch/pm-recorder-off-20261101/off-s3-20261101 s3`; if result receipts already exist, reuse them. Independently retrieve originals/root0600 hashes and Pi frozen logs/actual events. Verify one new deep cycle/Low-levelreturn/no checkpoint, exact preserved history and2→3 counters, no recorder activity/new EFI write, raw clocks/alarm/wake evidence, four timestamp domains, cleanup and full health. Physical result depends on the owner trace and interaction report; netconsole silence while NIC is suspended is expected.

Raw return precedes correction. Foreign alarm/settings are refused. Cleanup only owned settings when sleep inactive/MainPID0/no queuedsleep; successful cleanup is not repeated. On active sleep/unreachable/changedboot, bounded read-only evidence and owner notification only. Preserve a manual recovery boot separately, never treat it as return. No retry or further power workflow. This preparation archive remains immutable; append the later review separately.
