# Fresh serial Q35 with corrected verifier, October 30, 2026

Status: **ABORTED BEFORE SUSPEND: maintenance activation**. The final preflight refused an activating Nextcloud cron job at 14:12:57 CET. No logind suspend request or kernel sleep cycle occurred, and the corrected post-return verifier was not exercised. This is not HANG, SHORT_SLEEP or PASS. Its review is complete; subsequent owner-authorized Q37 preparation is tracked separately.

## Identity and schedule

- Boot: `3c1873e4-b866-41e1-b411-702168ef90cf`; btime `1793363873`, **13:37:53 CET**.
- System: `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`.
- Kernel: `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`.
- Modules: `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
- New runtime: `/nix/store/5gqrh1r71xnl127sj8dv83nnlk1nf6vs-predator-pm-study-0.1.0/bin/predator-pm-study`.
- Manifest: `preparation/plan.conf`, SHA256 `3d7c015927aec14d3bebac79f3ba0dc73df4f4f0a7e3106f853a5bc45e2f322e`.
- Suspend deadline: `1793365973`, **14:12:53 CET**, btime plus 2100 seconds.
- Wake: `1793366933`, **14:28:53**; retry `1793366963`, **14:29:23**.
- Probe: `1793367023`, **14:30:23**; snapshot `1793367113`, **14:31:53**.
- Review: **14:39**, rounded up from wake plus 600 seconds, 14:38:53.

Schema 2, Q range 1800..2400 seconds, ordinal 1, serial `pm_async=0`, trace 0, recorder off, deep, pm_test none, initialized TPM, raw EC not-run, logged-in users allowed with inhibitors enforced. Ordinary async is restored to 1. Manifest window 08:00 to 20:00 is within the owner-authorized 08:00 to 23:00 observation window. Scheduled 960 seconds does not itself prove 900 seconds asleep.

## Why this test

Both long-age first async D and serial D hung at owner-reported approximately 15 W on kernel 6.18.58. The original Q35 resumed on the same boot after 954.773 seconds; the owner reported approximately 3 W. Its early-snapshot verifier failed and wrote no completion, and its dated graph is still missing. Those observations establish neither a compliant reproducible passing bound nor an age threshold. The shared last pre-sleep r8169 netconsole callback cannot identify a culprit because Q35 returned after the same boundary.

The owner requested the next test and explicitly approved one graceful shutdown of the healthy machine plus one planned WoL startup. This trial tests the corrected verification path and the short-age serial condition under a controlled startup. The startup differs from original Q35's owner-forced reboot, and the instrumentation changed. Keep both differences explicit. Even a compliant Q cannot replace the successful D primer needed for E.

## Runtime change and build evidence

Only `pkgs/predator-pm-study/trial.rs` changed for this fix. The new runtime preserves raw `after-kernel`, then follows the exact expected boot's kernel journal for at most 30 seconds until the strict ordered current-cycle exit. It writes separate `verification-kernel` and `verification` artifacts, kills/reaps its direct journal process and joins its reader on every outcome, and retains all existing ordinal, identity, timing, duration, clock and storage guards. Missing or invalid history remains an error.

All **24 tests** passed, including four regressions for the captured late exit, a genuinely missing exit, exclusion of an earlier completed cycle, and misordered phases. `nix build .#predator-pm-study --no-update-lock-file` succeeded. Source copies/checksums, patch and build/test logs are preserved in `preparation/`. Old package `l423y5sbmkgcl50cjfkxwhqg1im7b31q` remains pinned for old evidence. The system generation, kernel, modules and system services were not activated or replaced.

Unprivileged closure copy refused the unsigned local package. Only the built package was then exported and administratively imported using the existing sudo permission; local/remote NAR hashes matched. No Nix trust setting changed. Local and remote runtime GC roots remain at `.scratch/pm-serial-q35r1-20261030/runtime-root`; keep the existing frozen-system root too. Controller SHA256 `56857827226a4e55319ffa69dec8ad269c3ed7ed7ff6a021f41915535901246c`; runtime SHA256 `6681936bb69ab828f41d7207282cd1cf95f1a4e155804a4b266bcb15a1805bea`.

## Controlled startup and preflight

Pre-reset health passed at 13:33; the only failed unit was the original Q35 verifier. Shutdown was acknowledged at **13:34:37**, and the old journal records System Power Off finishing **13:34:46.482383**. The root-only startup acknowledgement guard was created only after that acknowledgement. One Pi WoL ran **13:37:37.646648** and its service succeeded. Physical off-state and duration were not independently measured. The bounded read-only observer expired normally after 250 seconds; its process was reaped and the spent startup guard removed. No recovery wake or forced reset was performed.

At 13:39 the new system was still starting. A bounded readiness wait completed; full 13:40 health then passed with no jobs or failed units. All five authenticated disk and late-phase rejection audits passed. Baseline, TPM/EC/Acer settings, AC/lid/screen/WoL, declared LAN/DNS, fresh NTP, services/four healthy containers, pools/five mounts were correct. Counters were 0/0, recorder N/markers 0, async 1, trace 0, and no HOLD or maintenance. USB identities, serials, speeds and power-state snapshot matched original Q35 preparation byte-for-byte. Nightly/legacy-next timers stayed inactive.

Stage, harmless rehearsal and arm completed by 13:42:25. Independent checks at 13:43 confirmed the exact primary timer, new pinned attempt/restore commands, inactive trial service, zero cycles/markers and only this trial's future Pi timers. Pi receiver and NTP were active and the waker hash matched. Full baseline Pi log: **2,152,356 bytes**, SHA256 `267f547981c6fddd3832a5d545739844142cc7a2f85f040a3ab90fc032dfb313`. Telegram setup **88788** and owner schedule/graph request **88789** were acknowledged. Complete captures and checksums are in `preparation/`; disposable Cargo build products and the exported NAR stay in task scratch rather than the evidence archive.

## Review

Task `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q35r1-review-20261030-1439` was deleted at review start. Its previously verified next run was **`2026-10-30T13:39:00.000Z`**. The October 30 guard passed at **14:39:32 CET**. Collection and notification-status both exited 0. No baseline edits, recovery actions, delegates, commits/pushes or automatic follow-on suspend occurred during review.

Use the new pinned controller, correct WAN adapter, fresh capture directories and expected compact boot `3c1873e4b86641e1b411702168ef90cf`. Preserve full Pi result and all raw artifacts, including the early snapshot and new verification files. Require request Ok, expected boot/baseline, exactly one ordered deep cycle, counters 1/0, markers 0, current-cycle measured sleep at least 900 seconds, valid matching completion/last-completed, fresh NTP, healthy pools/five mounts, and a dated owner low-power graph for this trial. Prior Q35's 3 W report does not apply to this one. Inspect bounded-reader cleanup and retain partial errors; never synthesize completion.

If unavailable, preserve Pi/notifications and report suspected hang unless the owner confirms it. A recovery boot is not a resume. Request the dated **14:12 to 14:33 CET** graph. Classify first, then recommend the next justified action; no polling replacement schedule or automatic E.

## Actual attempt and classification

The trial service started **14:12:53.735506 CET**. CAPTURE CHECK carried the expected boot and nonce `1793365973`; initial preflight at **14:12:54.201586** reported 2100 seconds awake, prior successes 0 and markers 0. All three disk warmups completed. The second preflight emitted its common PM/health marker at **14:12:57.215254**, reporting 2103 seconds awake, but its subsequent maintenance check refused `nextcloud-cron.service loaded activating condition start`. A PREFLIGHT log line alone does not establish that every later schema-2 check passed.

The cron journal independently records Starting at **14:12:56.983699**, Started at **14:12:57.439108**, and successful deactivation at **14:12:58.381263**. Its `ExecCondition` checks Nextcloud status before running `cron.php`; the service did proceed and complete normally. The timer uses `OnBootSec=5m` and `OnUnitActiveSec=5m`. This was a real activation overlapping the final preflight, not a Nextcloud failure or evidence of a suspend hang.

The result, epoch `1793365977`, is `Err("request/entry/return failed: active maintenance: nextcloud-cron.service loaded activating condition start nextcloud-cron.service; clock=Ok(())")`. The error was logged **14:12:57.286249**. Cleanup at **14:12:57.310863** restored `pm_async=1` and ensured timesyncd was started. The abort preceded stopping NTP, BEGIN and the logind suspend request; `clock=Ok(())` is not a post-resume clock handoff. The failed trial unit is retained, with MainPID/ControlPID 0 and no active control group.

At 14:42 the same boot, frozen system/kernel/modules and new runtime binary hashes matched preparation. Suspend successes/failures were **0/0**, recorder N, markers 0, ordinary async 1 and trace 0. No system jobs or future study timers remained. NTP was synchronized, with packet count 7 and offset +3.197 ms; both pools, all five exact mounts, application services and four Immich containers were healthy. The only failed unit was this refused trial.

Independent analysis validated the expected boot ID in all **1221** full kernel-export records. BEGIN for this trial, SUSPEND REQUEST, deep entry, low-level resume, suspend exit, measured sleep and VERIFIED counts are all zero. The systemd-suspend journal has no entries. Retained files are only plan, requested, before, recovery and result. A `requested` manifest is written before preflight and is not proof of a submitted logind request. The following are absent: return, after-kernel, after-service, verification-kernel, verification, kernel-completed and completed. The new bounded journal reader was never reached; this attempt does not validate or invalidate its cleanup path. `last-completed.txt` still belongs to boot `5271820b-0d2c-4232-a00e-ebd0bf5c34d2`, epoch `1792350965`, October 18.

Classification: **`ABORTED_BEFORE_SUSPEND_MAINTENANCE`**. No sleep duration, completion anchor or new age-comparison result exists. Do not synthesize any of them or transfer the prior Q35's owner-reported 3 W to this attempt.

## Observer evidence and archive

Both planned WoL commands ran successfully at **14:28:53.668165** and **14:29:23.648530**. The machine had remained awake. The LAN probe succeeded at **14:30:24.004524**, and the frozen snapshot completed at **14:31:53.573937**. Empty properties from later garbage-collected transient units do not negate these journal-confirmed events.

The complete Pi result has **2,155,775 bytes**, SHA256 `49e883270e262898167ef6e9602f6143fea0be4ccece80371df6a6aaa18b6eaa`. At 14:42 the receiver was active, NTP synchronized and the waker hash unchanged. The isolated current attempt ends in cleanup, with no suspend request. Telegram acknowledgements: start **88791**, wake **88792**, retry **88793**, successful probe **88794**, snapshot **88795**, review **88796**. The review requested the dated **14:12 to 14:33** power trace even if flat; it remains missing, but cannot turn this no-suspend attempt into a PASS.

Full collector output, observer checks, original retained artifacts and tar, expected-boot kernel export, independent Rust analysis, notification acknowledgement and checksums are in `review-20261030-1439/`. Plan/requested SHA256 is `3d7c015927aec14d3bebac79f3ba0dc73df4f4f0a7e3106f853a5bc45e2f322e`; before is `1d0f9c53b2d0181239f77d801f5958f3dd4b5581d1f2d48366e640907ded3d8c`; result is `b40541f57713269918bb2514520b016bfc8ef1f71e838c3044e18f0088b9798e`. The first supplementary inspection mistakenly supplied a dashed journal boot ID and stopped; its partial output/error is preserved. The corrected compact-ID inspection and independent export completed successfully without changing machine state.

The newly collected notification-status text had the private recipient ID redacted before archival. Its audit retains original/new hashes and replacement count; message IDs, dates, bodies and outcomes remain intact. No token was present. Full Pi/technical evidence and all earlier captures are unchanged.

## Recommended next test

Keep the maintenance guard and corrected runtime. A new first serial **Q37**, due exactly **2220 seconds after a fresh documented boot**, would place the target away from the observed five-minute cron phase while remaining within Q's 1800..2400-second range. Check the actual timer forecast and maintenance state before arming; an active job must still refuse the test. This is an explicit age adjustment, not an exact Q35 replication, guard bypass or an age-bisection result. The current boot is already outside Q, so a delayed retry cannot supply this control. Any new trial needs a new ID, preserved startup provenance, its own >=900-second measured sleep and dated graph. No new reset or test was scheduled during this review, and Q cannot prime E.

After that review the owner instructed "automatically do that, always schedule the next test". The recommended [Q37 preparation](../r-serial-q37-20261030/README.md) was undertaken, but its single planned startup has not yet produced a reachable new boot. This is a separate startup blocker, not a change to Q35r1's no-suspend classification. A guarded Q37 preparation task is scheduled for 15:10 CET; no suspend is armed. The standing automatic-continuation policy is in the main study README.
