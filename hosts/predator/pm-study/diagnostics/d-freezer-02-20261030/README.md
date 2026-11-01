# Freezer retry, October 30, 2026

**Original result: DIAGNOSTIC_RETURNED_POSTCHECK_FAILED.** The freezer checkpoint returned after five seconds, but the immediate LAN post-check failed before reconnection and no completed record was written. Same owner-recovery boot `f8eb6e44-85ae-4e56-bc00-587876699945`, btime1793381192=18:26:32 CET, diagnostic ordinal1/freezer. The previous attempt stopped before a PM cycle; its [review](../d-freezer-01-20261030/README.md) preserves that self-job guard failure. A later owner-authorized revalidation at20:03:57 preserves this original failure and unlocks devices through a separate, explicitly typed predecessor record. No manual reboot was needed.

| Event | CET | Epoch |
| --- | --- | --- |
| Freezer diagnostic |19:23:32|1793384612|
| Pi read-only probe |19:25:32|1793384732|
| Pi full snapshot |19:27:32|1793384852|
| Review |19:29|1793384940|

Manifest `preparation/plan.conf`, SHA256 **91e1616b734ee77a0077bccd130cecfe5621dfa0fba8b2326207e1064a8f4484**, pins schema`pm-test-1`, freezer/ordinal1/predecessornone, serial0/trace0/recorderoff/deep/delay5/TPMinitialized/ECnot-run/inhibitors enforced, window18:00–23:00. Planned age3420s (57 minutes) is explicit diagnostic timing, not a real-S3 Q-age substitution. No900-second or3 W condition applies to a deliberately unwound diagnostic; no ordinary S3 completion/primer is created.

System `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`, kernel `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`, modules `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules` remain unchanged. Diagnostic package `/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0`, binarySHAf1a3981959b6ea5eae30fa1e668b62cb15c7c21620529bfd3e777b59c68944d9 and sourceSHAa5560085ebd86bf80a174d521146ea6bcefa20b0f1fba28262f3ac4fa6152bb2 are unchanged. Existing31 tests/build remain valid; no runtime edit or system activation.

## Explicit orchestration correction

The first primary used Type=oneshot, so its own start job made the strict empty-job check fail. Harmless paired rehearsals reproduced that behavior and showed an empty job list under Type=exec. This primary uses **Type=exec, RuntimeMaxSec=180s, TimeoutStartSec=30s, TimeoutStopSec=30s**, plus the same pinned run and safe-restore ExecStopPost. An explicit runtime limit is required because Type=exec's start timeout does not cover its running lifetime. No guard is ignored or weakened. Only the archived first attempt's explained failed-unit state was cleared.

Stage/check at19:11:36 and final arm check at19:12:26 passed on the expected boot with counters0/0, markers0, recorderN, pm_testnone/async1 and no requested artifact. Source/binary/kernel remained pinned. Actual cron ran19:11:38.094268 to19:11:39.486795; its five-minute/one-minute-accuracy phase projects before19:23:32, but runtime maintenance guards remain authoritative. No timer configuration was changed.

The Pi received expected-boot capture marker nonce1793383960. Full frozen baseline **2,456,189bytes**, SHA256 **391a7d6f5e41599e3b7cc609e6a8ed952e9300ec279ea14f71a657cce786d90e**. Only read-only probe/snapshot accompany this test. Independent19:14 proof confirmed primarywaiting/serviceinactive, Typeexec/runtime180/start-stop30, counters0/0, ordinary modes, no jobs/failedunits/requested artifact, NTPyes, and matching observer timers. Notice **88814**, epoch1793384060, acknowledged the prior abort and new schedule.

## Review and continuation

Exact task `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-freezer-retry-review-20261030-1929` was successfully deleted at review start. Its previously verified nextRunAt was **2026-10-30T18:29:00.000Z**; the October30 guard passed **19:29:13 CET**. Original artifacts, full journals and Pi evidence were collected into new paths.

Collect original root diagnostic artifacts, full expected-boot kernel/service journals, unchanged ordinary last-completed and full frozen Pi result `.agents-local/d-freezer-02-20261030-result.log`. Require the ordered five-second diagnostic cycle, successful logind/service return, reader cleanup, operation/cleanupOk, counters1/0/markers0, restored modes, healthy clock/storage/apps and a valid `DIAGNOSTIC_RETURN_VERIFIED` completion. Never synthesize one. Never clear pm_test during an active/queued sleep operation or override safe-restoration refusal.

Devices requires a valid completed-file predecessor. This executed cycle has none, so progression is blocked despite the observed return and current health. The zero-cycle retry exception does not apply. Preserve Typeexec/runtime-limit correction and the [bounded ladder](../README.md). No completed record was synthesized, runtime changed, failed status cleared, new test/polling task scheduled or reboot/wake performed in this review.

## October30 19:29 result

| Evidence | Observed value |
| --- | --- |
| Service start |19:23:32.643522|
| BEGIN |19:23:34.057632, reported age3422s, serial0/freezer/ordinal1|
| Kernel entry |source monotonic3421.213035, journal3421.430908|
| Five-second marker |source3421.273036, journal3426.482588|
| Kernel exit |source3426.395792, journal3426.613026|
| Systemd return / finish |19:23:39.392281 /19:23:39.402136|
| Raw return |epoch1793384619, same boot, request_and_returnOk|
| Reader teardown |stopOk, reapedOk with SIGKILL status9, reader_joinedtrue|
| Operation / cleanup |bothOk, modes restored before post-check|
| Post-check failure |19:23:39.819615, `LAN profile changed`|
| Completed |absent|

Independent Rust analysis validates all **1207** full kernel-export records against compact bootf8eb6e4485ae4e56bc00587876699945. Full history, observed-journal and immutable after-kernel each have exactly one ordered entry/debug-delay/exit, with no low-level resume or timekeeping sleep measurement. Journal ingestion of frozen-task messages occurred after thaw; source timestamps and full netconsole preserve the earlier five-second marker. Do not confuse the ingestion-time gap with the checkpoint duration. The kernel did not enter realS3. This successful checkpoint return does not rule out intermittent freezer failures and says nothing yet about the later device/platform/core phases.

All root0600 retained files were retrieved unchanged. Important SHA256 values:

- requested:2db6f800c48963e4b704c8cddc7d0815fb6058f48c56d352bcff0c598205db49, exactly the trimmed plan.
- observed-journal:65aa135f2cfe48cbc2d182833e5353d842e45e8f13c407c17bd0a18bfaf5566d.
- return:070da2b7b41bb3710b066bd006cc1ebed80b379cef7ed7b14e26304cdce0d5d5.
- outcome:52125d1722d78cb519292008895f3074d6794209524139263fbf0ffab63bf3c2.
- after-kernel:6fe6c20a0224cd98858c0a607a14ca5f41c4a21f3618037c2b62af6076532ff6.

Before, before-kernel and cursor are also present; the full inventory/tar and checksums are preserved. Ordinary last-completed remains the unrelatedOctober18 record, SHA280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08. The failed unit and original absence of completed remain intact.

### Network timing and current health

After systemd's return, Ethernet link went down at19:23:39.490196 and19:23:39.692240. The runner checked GENERAL.CON-UUID once and failed19:23:39.819615. Link came up at **19:23:43.203408**,3.384seconds later. At **19:30:57**, enp3s0f1 was connected with full connectivity, original UUIDc4e6863b-7f76-4a4d-9a48-9937af0b5a36, IP192.168.1.19/24 and DNS192.168.1.1. The profile timestamp1793384623 matches19:23:43. No network configuration was changed during this review. The rejected nmcli value was not captured; NetworkManager logging isWARN, limiting the exact activation timeline. Evidence supports premature post-return readiness checking rather than a lasting profile change. It does not identify the NIC as the cause of real-S3 hangs.

Current identity/binary/baseline match. Counters1/0, recorderN/markers0, trace0, pm_testnone/async1; systemd-suspend inactive/MainPID0, no jobs. Only this diagnostic unit is failed. NTP synchronized, both pools/five exact mounts healthy, required services active and all four containers healthy. No outstanding journal-reader process was observed. Safe cleanup ran again in ExecStopPost at19:23:39.902482 without overriding its inactivity checks.

Pi probe succeeded **19:25:32.981204**, snapshot completed **19:27:32.575804**. Full frozen result **2,458,179bytes**, SHA256 **c30e301b78646d48e0a2055bc3724ee108a026348d5fac2e3523d435519cb9a0**, identical to the live log at19:29:53, whose last write was19:23:39.435413217. It contains the checkpoint entry/delay/exit and stops before the link transition; use the persisted journal for later cleanup/failure evidence. Pi receiver/NTP remained healthy; no future test timers remain.

### Actual continuation blocker

Classification is **DIAGNOSTIC_RETURNED_POSTCHECK_FAILED**, not a freezer hang, real-S3 PASS or formally completed diagnostic. A reviewed bounded readiness check and an explicit same-boot continuation/revalidation protocol are needed to preserve this boot's value. They must retain strict UUID/DNS/health checks, capture deadline/mismatch errors, and preserve the failed original record and actual later validation time. Creating a retroactive completed file would violate the current protocol. Runtime edits were explicitly excluded from this review, so the ladder stops here with the healthy host kept running. No manual reboot is requested as a workaround.

Review notice **88815**, epoch1793385177, explained the result and software blocker. Complete evidence is in `review-20261030-1929/`; prior preparation and first-attempt archives remain immutable.

## Later authorized revalidation,20:03:57

After the owner said **"do that fully"**, the reviewed network-readiness fix and strict schema`pm-test-2` were built and tested (36tests plus Nix build/flake evaluation). Original artifacts were hash-checked again. Only this already explained inactive failed-unit state was cleared; no cycle or PM-mode change occurred. The new runner validated the original successful request/return/cleanup and sole LAN post-check error, exact same boot/history and full current health.

New `d-freezer-02-20261030-revalidated.txt`, SHA256`8ad223af8553390f307e19a84aa2b9f825fc9f0fb5d1a9d567d515696dd32e2c`, records result`DIAGNOSTIC_RETURN_REVALIDATED`, actual validated_epoch1793387037, original_return_epoch1793384619, original completion absent, artifact hashes and old/new runtime identities. The original `completed.txt` is still absent and ordinary S3 last-completed is unchanged. This is a new audited continuation record, not a retrospective replacement for the failed result. Devices ordinal2 consumes its hash with explicit `previous_kind=revalidated`; see [readiness/revalidation evidence](../readiness-revalidation-20261030/README.md) and [devices preparation](../d-devices-01-20261030/README.md).
