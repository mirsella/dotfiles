# Fresh serial Q35, October 30, 2026

Status: **armed, no result yet**. This is the only pending real trial. It tests first-suspend behavior at a short awake age under fixed serial mode on the unchanged kernel 6.18.58 baseline, after both long-age modes hung.

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

Review task `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q35-review-20261030-1302` is enabled with verified next run **`2026-10-30T12:02:00.000Z`**. It deletes its exact ID at start, including stale/early invocations, and permits machine actions only October 30 at or after 13:02 and before 23:00 Europe/Paris. Its saved prompt requires collection and classification, not an automatic further suspend.

Use the pinned controller and WAN adapter, fresh capture directories, expected compact boot UUID `8b7a173d84064072aebcc85c8b4989e7`, and full frozen Pi result. Full PASS requires request result Ok, matching boot/baseline, exactly one ordered deep entry/exit, counters 1/0, markers 0, current-cycle measured sleep at least 900 seconds, valid completion record, fresh clock sync, healthy pools/five mounts and dated owner low-power graph. The scheduled 960 seconds does not establish sleep duration. Missing graph is pending physical evidence; SHORT_SLEEP/timing/clock/storage deviation is not a compliant pass. SSH return or a recovery boot is not proof of resume.

If unreachable, preserve all Pi/notification evidence and distinguish suspected hang from owner confirmation. No recovery wake/reset/power toggle or security experiment. Q is not the successful D primer required for repeat E. Preserve all evidence, update the result and propose the next justified test only after review.
