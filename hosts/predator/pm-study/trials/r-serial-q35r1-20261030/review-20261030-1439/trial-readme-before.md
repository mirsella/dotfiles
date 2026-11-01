# Fresh serial Q35 with corrected verifier, October 30, 2026

Status: **armed, no result yet**. This is the only pending real trial. It repeats the short-age first-cycle condition with corrected post-return instrumentation and a documented controlled startup. No E or automatic next suspend is armed.

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

Task `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q35r1-review-20261030-1439` is enabled, with verified next run **`2026-10-30T13:39:00.000Z`**. It must delete itself at start even stale/early, and perform machine actions only October 30 at or after 14:39 and before 23:00 Europe/Paris. No baseline edits, recovery actions, delegates, commits/pushes or automatic follow-on suspend during review.

Use the new pinned controller, correct WAN adapter, fresh capture directories and expected compact boot `3c1873e4b86641e1b411702168ef90cf`. Preserve full Pi result and all raw artifacts, including the early snapshot and new verification files. Require request Ok, expected boot/baseline, exactly one ordered deep cycle, counters 1/0, markers 0, current-cycle measured sleep at least 900 seconds, valid matching completion/last-completed, fresh NTP, healthy pools/five mounts, and a dated owner low-power graph for this trial. Prior Q35's 3 W report does not apply to this one. Inspect bounded-reader cleanup and retain partial errors; never synthesize completion.

If unavailable, preserve Pi/notifications and report suspected hang unless the owner confirms it. A recovery boot is not a resume. Request the dated **14:12 to 14:33 CET** graph. Classify first, then recommend the next justified action; no polling replacement schedule or automatic E.
