# Fresh serial Q35, October 30, 2026

Status: **same-boot resume confirmed; verifier error; dated physical evidence pending**. The kernel measured **954.773 seconds asleep**. The runtime rejected an incomplete early journal snapshot, so no valid completion record exists. This is not a hang and is not a compliant PASS. No trial remains pending.

## Identity and schedule

- Boot: `8b7a173d-8406-4072-aebc-c85c8b4989e7`; btime `1793358004`, **12:00:04 CET**.
- System: `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`.
- Kernel: `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`.
- Modules: `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
- Runtime: `/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study`.
- Manifest: `preparation/plan.conf`, SHA256 `47b7709ece14ca45d55d67cea044434153bf3ca39f251b7af57db67c950ccbf5`.
- Suspend deadline: `1793360104`, **12:35:04 CET**, 2100 seconds after btime.
- First WoL: `1793361064`, **12:51:04**; retry `1793361094`, **12:51:34**.
- Probe: `1793361154`, **12:52:34**; frozen snapshot `1793361244`, **12:54:04**.
- Review: **13:02**, rounded up from wake plus 600 seconds, 13:01:04.

Schema 2, profile Q (1800..2400 seconds), ordinal 1, `pm_async=0`, trace 0, recorder off, deep, pm_test none, initialized TPM, raw EC not-run, logged-in users allowed with inhibitors enforced. Manifest window 08:00 to 20:00 is a subset of the authorized 08:00 to 23:00 Europe/Paris window. The runner restores ordinary `pm_async=1` after preflight and completion.

## Provenance and question

The owner force-rebooted the serial D hang and reported about 15 W. This boot is recovery, not D resume. Exact reset/off duration and startup method were not supplied. D's preceding controlled shutdown/WoL differs from this owner's forced-reboot startup; preserve that confound. No new reboot was requested for Q.

Both long-age modes have now hung, so async callback execution is not necessary for the observed failure. Q asks whether serial suspend can meet all completion and physical-sleep criteria at a short age on this baseline. One pass would support reproducibility/reset-matching checks before age testing. One hang would weaken a long-awake-only explanation. Neither establishes a sharp age threshold, and the final logged NIC callback does not identify the culprit.

The [Q30 draft](../r-serial-q30-20261030/README.md) was aborted before primary arming because the boundary projected 1799 seconds. All its observer timers were canceled, and no requested artifact exists. This fresh Q35 manifest avoids the boundary without changing a guard or substituting an already-executed trial.

## Preparation evidence

Health checks at 12:04 passed the exact baseline, all five authenticated boot and late-phase rejection audits, TPM/EC/Acer checks, AC/lid/screen/WoL, fresh NTP/LAN, services/four healthy containers, pools/five mounts, zero successes/failures and markers. No HOLD, jobs, failed units or checked maintenance remained. USB identities, serials, speeds and power-state snapshots matched serial D byte-for-byte. Those shared pre-arm checks are retained in Q30's immutable preparation archive. Pi NTP, receiver and waker hash passed.

Q35 stage, harmless rehearsal and arm succeeded at 12:09. Its full baseline Pi log contains **1,979,219 bytes**, SHA256 `1e9a5991f0a712ee4f4ac33981f70862259638a1366a2bb65ed4deec1369471a`. Independent 12:12 checks verified the exact primary and Pi timer times above, nonpersistent timers with 1-second accuracy/no random delay, primary inactive, zero cycles/markers and ordinary `pm_async=1`. Nightly, legacy-next and canceled Q30 timers were inactive. Telegram setup **88778** and corrected owner summary **88779** were acknowledged. Complete captures and checksums are in `preparation/`.

## Review

Review task `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q35-review-20261030-1302` had verified next run **`2026-10-30T12:02:00.000Z`** and was deleted at the start of this review. The October 30 time guard passed at 13:02:27 CET. Collection and notification-status completed successfully into fresh directories. No follow-on suspend or replacement review task was scheduled.

Use the pinned controller and WAN adapter, fresh capture directories, expected compact boot UUID `8b7a173d84064072aebcc85c8b4989e7`, and full frozen Pi result. Full PASS requires request result Ok, matching boot/baseline, exactly one ordered deep entry/exit, counters 1/0, markers 0, current-cycle measured sleep at least 900 seconds, valid completion record, fresh clock sync, healthy pools/five mounts and dated owner low-power graph. The scheduled 960 seconds does not establish sleep duration. Missing graph is pending physical evidence; SHORT_SLEEP/timing/clock/storage deviation is not a compliant pass. SSH return or a recovery boot is not proof of resume.

If unreachable, preserve all Pi/notification evidence and distinguish suspected hang from owner confirmation. No recovery wake/reset/power toggle or security experiment. Q is not the successful D primer required for repeat E. Preserve all evidence, update the result and propose the next justified test only after review.

## Observed execution and return

The runtime preflight reported 2100 seconds awake at 12:35:04.803897 and 2103 seconds at 12:35:07.365465, with zero prior successes and markers. BEGIN was 12:35:07.597944; logind SuspendWithFlags request was 12:35:07.743444. The expected boot and pinned generation/kernel/modules were unchanged on return and at review. The later complete kernel journal has exactly one entry at journal monotonic 2103.802681, low-level resume at 2109.838989, measured sleep **954.773 seconds**, and exit at 2109.947126. Exported source timestamps are preserved separately from journal timestamps.

The raw return artifact records `observed_epoch=1793361068`, expected boot, `request_result=Ok(())`, successes/failures **1/0**, and markers **0**. Systemd recorded return at **12:51:08.633839**. The recorded return is within the planned wake window; it precedes NTP recovery and is not a completion timestamp. Recovery diagnostics record `request=Ok(())` and `clock=Ok(())`, fresh initial synchronization at 12:51:39, both pools healthy and all five expected datasets mounted. The 13:05 review confirmed those mounts and pools, current NTP, counters 1/0, unarmed recorder/zero markers, trace 0 and restored ordinary async 1. Core application services and all four Immich containers were healthy. The socket-activated Podman API was inactive with result success and its socket listening, not an application failure. The only failed unit was this trial's verifier; it was left intact as evidence.

Pi journals confirm first WoL **12:51:04.653384**, retry **12:51:34.648609**, successful LAN probe **12:52:34.847423**, and completed snapshot **12:54:04.568672**. Empty properties for already garbage-collected transient units do not negate their execution journals. The full frozen result contains **2,012,555 bytes**, SHA256 `c5ced61d41140d16387ec10c2e61a7065d485f7c052ee0da4e122579cea16761`. Its trial-specific excerpt stops before sleep at `r8169 ... calling pci_pm_suspend` and resumes with link-up at 12:51:12. This same pre-sleep boundary occurred in D, but Q returned; it cannot identify D's failing driver. The Pi was still synchronized and receiving with the correct waker hash at review.

## Verifier failure and classification

The retained `after-kernel` file has 2063 lines, one entry, one low-level resume and no exit. It was saved at **12:51:08.802156083**, ending during resume callbacks. SHA256: `57149c2e764968960ed7b628529326bc9435b21e32f7115d624a8f2be4b7384f`. The exported journal records receiving `PM: suspend exit` at **12:51:08.831444**, about 29 ms after that file was saved. The later complete journal contains the missing exit and one current-cycle duration above 900 seconds.

The deployed-source checksums match the inspected `pkgs/predator-pm-study/main.rs` and `trial.rs`. `raw_return()` immediately captures `journalctl -k` after the systemd return message; `attempt()` later verifies that same saved snapshot after clock recovery. It does not acquire a complete kernel snapshot before `completed_cycles()`. Thus the service returned `Err("unfinished suspend in kernel history")` at **12:51:40.523289**, although the kernel cycle completed. The raw result is preserved, not relabeled as success.

There is no `kernel-completed` or `completed` artifact. The unchanged `last-completed.txt` still refers to boot `5271820b-0d2c-4232-a00e-ebd0bf5c34d2`, epoch `1792350965` from October 18. No completion record was synthesized and no `sleep_seconds` completion field or valid completion anchor exists. Classification is **RESUMED_VERIFIER_ERROR_PHYSICAL_EVIDENCE_PENDING**, not full PASS, SHORT_SLEEP or HANG. The dated owner graph for **12:34 to 12:55 CET** remains requested. Even with a graph, this Q is not E's required D primer.

Telegram acknowledgements: **88780** start, **88781** first wake, **88782** retry, **88783** successful probe, **88784** snapshot and **88785** review. The review notice reports the verifier failure, measured duration and graph request. Complete collection, notification records, raw artifact tar and extracted originals, full boot journal export, independent analysis and checksums are preserved under `review-20261030-1302/`; preparation evidence remains unchanged.

## Recommended next action

Correct the verifier's kernel-journal capture race before another trial: preserve the early raw return snapshot, use a bounded boot-bound wait for the ordered kernel exit, and verify a separate complete snapshot without weakening the ordinal, timing, duration, clock or storage requirements. Exercise that change against delayed journal delivery and a genuinely missing exit using this captured evidence. This review made no code or baseline change; a future corrected runtime requires explicit instrumentation-baseline documentation.

Then repeat a fresh first serial Q35 with documented comparable controlled startup and a dated low-power graph. Q's observed resume supports testing a short-age passing condition, but the verifier failure, missing graph, single observation and reset-provenance difference prevent treating it as a reproducible age bound. Do not start age bisection or a same-boot E from this record. No further suspend, reboot, recovery WoL, power toggle or polling task was scheduled.
