# RTC-only deep S3 prepared, November 1, 14:06 CET

One real-S3 attempt is armed after the independently reviewed core recorder control. The owner requested suspend-hang investigation with redundant Pi logging, confirmed “Ready now” for local recovery/power recording, and received these exact times before arming. No return is claimed at preparation time.

| Event | Europe/Paris CET | Epoch |
| --- | --- | ---: |
| Entry target |14:20|1793539200|
| Owned absolute RTC alarm |14:25|1793539500|
| Pi bounded SSH probe |14:26:30|1793539590|
| Pi frozen netconsole |14:28|1793539680|
| Self-deleting read-only review |14:30|1793539800|

The dated14:18–14:30 power trace was requested. There is no WoL, automatic power retry or subsequent power-test chain. RTC wake is proven in s2idle but not yet in this real-S3 test; local recovery remains available. If unreachable or sleep is executing, only bounded collection/owner notification is allowed. Do not kill sleep or remove wake coverage.

## Controlled difference and predictions

The functional difference from the successful recorder control is `pm_test=core`→`none`, entering actual ACPI S3. The serial setting remains async0, ordinary device settings are retained, and the same tested module is used in its bounded S3 mode. Its three ordered64-byte writes mark phase20 immediately before the ACPI low-level call, phase21 immediately after its return, then phase22 after syscore resume. Each requires context7 and a fresh sequence; failures disarm rather than retry. The absolute RTC alarm is independent wake provision, written/read back immediately before the suspend request. Normal UTC/NTP, old RTC recorderN, trace0 and frozen security/system/kernel/modules remain required.

No sustained low power with phase20 as last evidence narrows final platform entry; physical low power without recorded low-level return after alarm leaves wake delivery/earliest resume unresolved; phase21 without22 narrows syscore return; phase22 before a later stall moves investigation into later resume. A successful RTC-backed return demonstrates this S3 path can work under these conditions. Missing records may indicate recorder failure, and untimed post-return PCI registers are not in-sleep packet proof. A changed boot after owner recovery is never a successful return.

## Distinct reviewed protocol

Trial`s3-efi-v2-20261101`, nonce`deep1101a`; state `/var/lib/predator-pm-efi-s3-reviewed`. Root0600 manifest `s3-efi-v2-20261101-plan.conf`, SHA256`bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c`. Service`predator-pm-boundary-s3-efi-v2-20261101.service` is Typeexec/Restartno/RuntimeMax900s/TimeoutStop180s with pinned ExecStopPost. At14:05:49 it was inactive/MainPID0 with timer14:20, over14minutes lead. Pi observer timers were installed14:05:48.

Expectedboot`39d7579b-b86d-4d30-b910-131943e550c8`, reviewed wall-clock btime estimate1793535079. The previous core harness's exact btime check failed after a one-second NTP correction despite unchanged boot UUID and completed cleanup. This runtime requires the same UUID and permits only an explicit, logged ±2second estimate difference; a different UUID or larger drift fails. Snapshots now include `/proc/stat`. It pins the separate independent core review SHA256`78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896` and seven critical original core artifact hashes, re-verifies the core record/status, and requires the original completion remain absent. It does not rewrite the old runtime or manufacture its completion.

Before S3, exact history is one deep diagnostic core cycle, no real low-level return, success1/fail0/RTCmarkers0. Expected successful cumulative result is2/0. Fresh health checks before preparation14:03 and arming14:05 passed frozen identities/security/five boot audits, original hashes, ordinary PM/sleep unit/config, clocks/RTC, LAN/DNS, pools/mounts/apps/four containers, all HOLD and maintenance/builder/job checks. Execution repeats them.

Pi boot2142644e-84db-40ae-b1b9-9e8c71265cf7, synchronized stratum2 NTP, active receiver and exact new nonce were verified14:03:44. Offset−216µs/delay9.153ms/jitter1.170ms. Independently retrieved baselineSHA256`1c6b69b7ca5f33d7a31d400ac42acaf365403dcbbf103952de1999e0679588e5`. Pi captures UDP netconsole and separate probe/snapshot receipts; it cannot see the NIC-off interval. EFI records and local journal cover different boundaries.

## Pins and evidence handling

- Runtime`/nix/store/2bis3s5m9djibh5fjgrjknpwah6w60rv-predator-pm-study-0.1.0/bin/predator-pm-boundary-s3`, SHA256`810564d7fc6d6ab3331220ce07e9c9a9959debde3d8299179c106881bef23e3b`; source`5c0e3e70286d63c60d73373239fc284a1872b11c63e6df0c7304f0f70a20c7bb`.
- Unchanged module`/nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz`, SHA256`90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0`; C source`aeeba42b27ef3c2448626e9557bb14494190b51833c63e78c81dba8610a537e7`.
- Shared record/timing source`8699c8457d330df8341b529e13cdaf437e50fd1035c576cbd98b4d8e89cdc2e7`; original core runtime source4884bb9d16b073785e441a17f807a6ce130f53e6b43a0a0cbdb9e41cd0a19f7f remains unchanged.
- Eight focused tests passed, including bounded clock-estimate handling, strict reviewed-core anchoring without relabelling its failure, manifest/history, cleanup, CRC/sequence/context/capacity and four separate clocks. Nix build and flake evaluation passed. New runtime GCroot and all previous roots retained; no system activation.
- The stage requires free space≥6912 before its three-write sequence, preserves Linux reserve5120 plus1024 margin and≤256 reported bytes per write. Last reviewed free7034 after core. The load-time check repeats immediately before mutation. Nothing deletes EFI variables or triggers garbage collection.
- Raw return precedes helper correction; snapshots include IRQ/fixed events/GPE69/wakeup sources/PCI/RTC and all four journal timestamp domains. Successful cleanup disarms/captures/unloads only the owned module and clears only its owned alarm after sleep is inactive/MainPID0/unqueued, then restores none/async1 and tested clocks. Do not repeat completed cleanup.

Local trial `.scratch/pm-s3-rtc-20261101/s3-efi-v2-20261101`; pinned operator `.scratch/pm-s3-rtc-20261101/operator-s3 collect TRIALDIR s3` is once-only. Its review-schedule.json stores exact reviewerID`scheduled-task:command:mcp:b1149009-6b28-4c5e-ae12-47335ef3ad69:schedule-task:predator-efi-s3-review-20261101-1430`, enabled/current-thread-bound, nextRunAt2026-11-01T13:30:00.000Z. Reviewer first deletes that exact ID, then collects and independently verifies; it schedules no further power test. Preserve retrieval failures rather than declaring artifacts absent.
