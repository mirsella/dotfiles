# One s2idle WoL test with RTC-alarm fallback

Later owner report: “it went to 3w.” This confirms the observed low-power level without supplying exact duration or the dated graph. The original review archive remains sealed. The [November1 follow-up review](../wake-path-20261101/review-1030/README.md) verified another RTC-window return despite bridgeD0/NICD3hot and PME/ACPI arming; the owner reported approximately6W. Cleanup passed. An active scrub and lack of a justified next wake setting left the second slot unarmed, with no further power workflow scheduled.

## Reviewed result

The [22:45 read-only review](review-2245/README.md) verified all20 original artifacts and one genuine s2idle return on the same boot. Both Pi WoL sends ran; return occurred at22:35 in the RTC-fallback window. Verified alarm expiry, IRQ9/ACPI fixed-event wake and RTC-source activity strongly support RTC fallback, although the exact fixed-event status bit was not captured. Pre-correction RTC and realtime agreed, cleanup succeeded, and fresh health passed with counters1/0 and markers0. The dated22:13 to22:40 power graph remains required for physical duration/full PASS. The review task below was deleted at review start; the schedule and preparation evidence are historical. No further power operation was scheduled.

The owner authorized fixing the previous low-power/no-WoL result and scheduling the next tests. The dated21:45 workflow read and deleted its exact task before the21:45:19CET date guard. Connectivity recovered; fresh21:45 health and22:01 pinned preflight passed. No earlier recovery collection or diagnostic ladder was repeated.

## Armed schedule

One primary was armed **22:05:20–22:05:37CET**, with more563seconds lead. At arming the primary was inactive and no requested artifact existed.

- Entry: **22:15CET**, epoch1793481300.
- Pi WoL: **22:31**, epoch1793482260; retry22:31:30.
- Independent RTC fallback: **22:35**, epoch1793482500.
- Pi bounded LAN probe22:36:30; frozen snapshot22:38.
- Read-only review: **22:45**, epoch1793483100.

Exact enabled current-thread review task:

`scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-wol-rtc-review-20261031-2245`

Verified nextRunAt`2026-10-31T21:45:00.000Z`. It reads the trial's `review-schedule.json`, deletes this exact task first and enforces October31/08..23. This review grants no further power operation.

Telegram notification failed with a20-second DNS timeout. The exact entry/WoL/retry/RTC/review times and dated22:13–22:40 graph request were delivered in the current task thread before arming. The operator now explicitly accepts that truthful thread receipt; the original Telegram-only operator/source/hashes and failed delivery are retained. No Telegram success was fabricated.

## Explicit runtime and history

Trial`wake-20261031`, schema`s2idle-wol-rtc-1`, separate state`/var/lib/predator-pm-wake-validation`. Root0600 manifest SHA:

`767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942`.

Boot`f9b45aa6-b820-4ab5-b18c-9324d8d53087`, observed/historical btime1793474520. Before this trial: zero diagnostics/S3/s2idle cycles, counters0/0 and markers0. Planned age since recovery boot is6780seconds. Exact owner button sequence/off duration remains unknown. The oldae29 boot retains its local-event s2idle return and interrupted restoration; the olderf8eb boot retains ten diagnostics and one15W failedS3. Neither is relabeled.

Runtime:

`/nix/store/y5x8nw8lvhjjmxp9m1aiy4hkgsfsrr7g-predator-pm-study-0.1.0/bin/predator-pm-wake-validation`

- Binary SHA`743a5546ccf6e13d401de82c934691efb75b92c9e4103e12a1c15091966086fc`.
- `wake_validation.rs` SHA`6074d8e9863e8d35819a85a84222551cef7c8c3344209f2aaffaa283ab7b4040`.
- `wake_timing.rs` SHA`d84db2b41c3f89279938dcc959690d5f41e6a619848dfebeb3864ca31d9f4648`.
- Operator source SHA`e82a889850a873a16b6a943cfe48fba3b7344dfdf2593685de5ab7dd3f36d1af`, binary SHA`fe934f8f0ca76317907a0225d1f360b6df0071d20b162ead5aa727cf9d837871`.
- Local/remote GCroot `.scratch/pm-s2idle-rtc-fallback-20261031-2145/runtime-root`.

The frozen system`zbq64a1n4yzr3knpsgzhiw29lvgs3b34…`, kernel`82haph8j8hmbny68rzkis8dqrz9gqrr5…`, modules`1qbmxim06wak3qplmj747hb5gkyzgj9z…` and recorder module are unchanged and fully pinned in source. R3 instrumented, diagnostic and previous s2idle source hashes still match. The separately rooted qzs…clock helper has SHA`b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b`; no system activation, trust-policy or persistent configuration change was made.

All **70 package tests** passed in the successful Nix build: control2, diagnostic14, instrumented8, previous s2idle9, main26, new wake-validation11. New tests cover distinct strict manifest/history/modes/deadline, owned-alarm-only cleanup, active/queued cleanup refusal, timing-window boundaries, source versus receipt timestamps, missing-source rejection, genuine s2idle timekeeping messages, wrong boot/deep/fake service rejection and late kernel exit delivery. Flake evaluation passed. An initial explicit-Result type error and cache DNS delays remain in failed build logs. A timed-out first package import left no owned process and no valid package at inspection; compressed package-only transfer then succeeded. This is software verification, not a hardware outcome.

## Valid clocks and independently programmed fallback

This runtime never arms the RTC phase recorder or sets STA_UNSYNC. It keeps normal UTC/NTP/RTC, trace0, pm_testnone, serialasync0 and temporarymem_sleep=s2idle. The effective systemd262 sleep configuration is pinned and cannot silently selectdeep. RTC capability inspection showed rtc_cmos wakeupenabled, correct October31 UTC within two seconds of realtime, and empty wakealarm. Stale disabled alarm-register dates in `/proc/driver/rtc` were not treated as an active alarm.

Immediately before logind request, the runner requires no existing alarm, persists `alarm-intent`, writes the absolute fallback epoch to rtc0/wakealarm, verifies the exact readback and persists `alarm-armed` with RTC/driver/wakeup-source evidence. **The RTC alarm is not claimed already armed at timer installation.** Failure to verify it aborts before a suspend request. The runner records true requestOk, follows JSON kernel/service messages and captures the return snapshot before any clock correction. It checks RTC/realtime coherence again before the helper can repair a clock error.

Cleanup only runs with systemd-suspend inactive/MainPID0 and no queued sleep/suspend jobs. It preserves pre-clear alarm/IRQ/wakeup/clock evidence, refuses to clear a foreign alarm, clears only this manifest's owned alarm if still present, restoresdeep/async1/none/trace0/recorderN, then uses the tested event-driven LAN/fresh-NTP helper. A fired/empty alarm is allowed. Never kill an executing primary or remove wake coverage without proven request cancellation. Primary is Typeexec, RuntimeMax2400seconds, TimeoutStop180seconds and pinned ExecStopPost restore; observer jobs are Typeexec with30-second limits.

## Independent observer and health

Pi boot`2142644e-84db-40ae-b1b9-9e8c71265cf7`, active receiver, no pending conflicting jobs/timers, synchronized NTP with normal leap/stratum2, offset+322µs/delay9.615ms/jitter976µs at22:02. Exact nonce`wake-20261031-1793481300` reached live capture and the frozen baseline. Baseline SHA:

`fd60ed790026c2f21c602a04069d5710d4185c0c26d114b0e800642a732ce363`.

Waker SHA`ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8` matched at preparation and arming. All four Pi timers were verified waiting. Full Predator preflight includes frozen identities, all five authenticated boot/late-rejection audits, TPMCRB/rawECnot-run/Acer exclusions, AC/lid/fb/WoLg, declared LANUUID/DNS/freshNTP, pools/five mounts/apps, noHOLD/jobs/failures/builders/maintenance and fresh history. Actual cron was21:57:04–04, then22:02:03; next22:07:03 at arming, projected22:12/22:17 brackets the22:15 entry. Runtime exclusion remains mandatory.

## Read-only review and interpretation

Collect once with the pinned scratch operator. Independently verify original root0600 artifacts, actual alarm readback, requestOk, kernel entry(s2idle)/idle loop/resume/exit and systemd return/finish, then ordinary restored modes/clocks/health. Reject deep/checkpoint/ACPI low-level return. Genuine Timekeeping-suspended messages are allowed because cpuidle/tick_freeze also suspends timekeeping in s2idle.

The JSON validator preserves `_SOURCE_MONOTONIC_TIMESTAMP`, `_SOURCE_BOOTTIME_TIMESTAMP`, `__MONOTONIC_TIMESTAMP` and `__REALTIME_TIMESTAMP` separately. Missing source timestamps fail rather than falling back to receipt time. Window classification is EARLY_RETURN, WOL_WINDOW_RETURN, INTERMEDIATE_RETURN_UNKNOWN_CAUSE, RTC_FALLBACK_WINDOW_RETURN or LATE_RETURN; these are not wake-cause claims. Correlate Pi send times, alarm expiry, wake IRQ/ACPI/source evidence and coherent UTC. Expected successful counters1/0 and unchanged markers0 are not asserted before review.

No S3PASS/ordinary primer or physical-duration claim follows from timing alone; the dated power graph remains required. If both wake routes fail, capture bounded evidence and request owner recovery; no extra WoL/reset/watchdog/crash/poll/repeat. Retrieval failure is not artifact absence. Preserve all earlier RTC originals and missing STATE/PLAN history.

`preparation/` is the checksum-covered pre-entry snapshot, excluding binaries/build caches. The guarded sync also includes the previously deferred12:33 review, evening recovery evidence and clock-readiness source fix after fresh22:06 remote status/flake.lock/preimages matched. A separate installer receipt records whether that transfer completed; no whole-tree synchronization, commits or pushes are used.
