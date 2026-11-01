# Evening recovery: s2idle returned, scheduled WoL did not

The owner reported: “force rebooted predator. it was at 3-1w, weird. what happened ? fix it, and schedule next tests”. Exact button sequence, off duration and a dated power graph were not supplied. The earlier12:33 inability to retrieve artifacts was not proof of their absence.

## Recovered result

Old boot`ae29f675-9167-4975-a884-01885468bc38` retained thirteen root0600 trial files. RequestOk is true. The return reader recorded success and joined; raw RTC before writers was2021-04-10 00:00:01UTC, action17/value1/startN, markers26, clock status8257. These are the expected s2idle final-console marker, not the S3 marker. Original manifest SHA remains`f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820`.

Kernel history contains ordered s2idle entry, idle loop, IRQ9 wake, ACPI Notify wake, loop return and exit. Systemd recorded return and Finished System Suspend. Logind recorded “Lid opened.” and “Power key pressed short.”; Linux subsequently reached Finished System Power Off and filesystem synchronization. This establishes a return associated with local wake events and an orderly shutdown, not an unrecovered s2idle hang. It does not isolate the failed WoL route or a driver.

Shutdown stopped the trial service and NetworkManager while cleanup was activating the LAN. The helper timed out after30seconds. There is no retained outcome, restored or completed artifact. The original return record is present and complete; it must not be replaced with a synthesized successful completion. The old boot has one s2idle request/return, zero S3 and zero diagnostic cycles. Earlier boot`f8eb6e44…` still has ten diagnostics and one owner-confirmed15W failed S3 attempt.

## Clock-domain error and source correction

The original reader subtracted `journalctl -o short-monotonic` receipt timestamps for buffered kernel messages. The two receipt timestamps are4389.352062 and4389.352183 seconds, yielding the retained but misleading0.000121s result. The same messages' `_SOURCE_MONOTONIC_TIMESTAMP` values are4385627438 and18943800437 microseconds, yielding **14558.172999s**, about4h02m38s in the kernel idle loop. Those fields must remain separate. The source interval is not a dated measurement of power draw or uninterrupted physical sleep.

The old uncorrected `pre_restore_epoch=1793444874` also cannot classify a real early wake. Matching Linux6.18.58 source now establishes the missing path: `drivers/cpuidle/cpuidle.c` calls `tick_freeze()` before `enter_s2idle()` and `tick_unfreeze()` afterward. In `kernel/time/tick-common.c`, the last CPU calls `timekeeping_suspend()` and the first returning CPU calls `timekeeping_resume()`. S2idle bypasses the deep/syscore branch but does **not** avoid timekeeping suspension. The earlier preparation explanation and old runner's blanket rejection of Timekeeping-suspended messages were overbroad.

The RTC phase logger disables the persistent/RTC fallback clock reads, as established in the earlier RTC assessment. It therefore also interferes with s2idle clock recovery. The next protocol keeps the RTC recorder off and accepts valid timekeeping evidence for an otherwise verified s2idle path. Old sources, binaries, results and failure records remain unchanged.

## New boot and retained RTC

New boot`f9b45aa6-b820-4ab5-b18c-9324d8d53087`, observed btime1793474520, corresponds to20:22:00CET. This is a clock-derived boot start, not the owner's exact button timestamp. Original root0600 bootRTC file is101939bytes, SHA256:

`ec6d9a401a59064512c56c035894715d4123ef69ee301cc83297a373c9a83b7d`.

It captured2021-04-10 04:11:29UTC before NTP; naive Magic is1:775:160. The unchanged tested calendar model has one candidate within24hours: final console marker action17/user1 plus15089seconds. This corroborates a retained final return marker under devicehash0, continuous RTC and no intervening clock-write assumptions. It is not15089seconds physically asleep. The older recoveryRTC SHA`b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55` remains intact.

All fourteen retrieved originals, including the new bootRTC, match authenticated remote digests. The independent read-only Rust verifier checks the original request/return/RTC fields, source/receipt intervals, local wake and poweroff records, and calendar decode. Full raw old-boot export and kernel JSON remain in scratch with hashes; selected journals, source-event TSV, originals and verifier are archived here. The frozen recovery Pi capture is separate from the immutable12:26 result.

## Clock-handoff repair

This new boot's clock-handoff service initially failed its immediate LAN-readiness check. The declared profile later became connected with correct gateway/DNS and NTP, leaving an explained failed unit. At21:07, after verifying inactive sleep/MainPID0/no jobs/recorderN and correct connected profile, restarting the existing frozen unit completed successfully and restored fresh NTP/RTC. The pre-repair failure and repair logs are preserved.

Source change: `pkgs/predator-pm-study/network.rs` waits on NetworkManager property-change events, checks the declared UUID, connected state, exact gateway and DNS, and performs one final deadline snapshot to cover subscription races. It rejects wrong profiles/failed states; no periodic polling or configuration fallback. `main.rs::clock_handoff` uses this readiness check before clock repair. Four new regression tests cover activation, wrong profile/routes, event/missed-event readiness and deadline failures. All59 package tests, Nix build and flake evaluation passed.

New helper:

`/nix/store/qzs6qmgvvka19nv4dyihz9rc8mbybkkv-predator-pm-study-0.1.0/bin/predator-pm-study`

- Binary SHA`b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b`.
- network.rs SHA`0a28b1851b4fe42d13d921a2467c4d6d45c831d128c5b502a66971b6135930dd`.
- main.rs SHA`515902c329c1280755e267094956c892ceb47a65753618998ceacf83afea5b85`.
- Local and remote GCroot `.scratch/pm-s2idle-recovery-20261031-2058/clockfix-runtime-root`.

It was imported without system activation and ran successfully at21:14:49, including event-monitor cleanup and fresh NTP at21:14:49.795158. The frozen boot unit still references its previous executable; this new helper is pinned for the next trial rather than silently replacing the system generation. The import's initial timeout, local DNS failure, failed cancellation guard and corrected known-PID cancellation are retained. Closure comparison proved only the new package needed transfer; its subsequent package-only import succeeded. Initial unavailable `lspci`, empty `pm_wakeup_irq` and first smoke-command path error are retained too.

Full health at21:17 passed: same frozen system/kernel/modules, all five authenticated boot and late-rejection audits, zero suspend counters/markers, normal deep/none/async1/trace0/recorderN, AC/lid/fb/WoL, declared LAN/DNS/fresh NTP/RTC, healthy pools/five mounts/apps, no jobs/failed units/builders/maintenance or future study timers. Nextcloud's current cadence is nearxx:02/07/12/17/22:03; recheck before arming. Current boot has had no power transition.

## Next test scheduled, not yet armed

Exact task:

`scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-s2idle-rtc-fallback-20261031-2145`

Enabled/current-thread-bound, verified nextRunAt`2026-10-31T20:45:00.000Z`: **21:45CET October31**. It reads `next-test-schedule.json`, deletes this exact ID and validates the date/window. This is preparation, not a hard suspend deadline. One distinct tested Rust s2idle protocol must preserve all old histories and pin this boot, runtime and manifest. It keeps serialasync0/pm_testnone/trace0, but the RTC phase recorder remains **off** so correct UTC, clock recovery and an RTC wake alarm can coexist.

After full health, RTC alarm support/readback, independent Pi NTP/receiver/nonce/waker proof and more300seconds lead: Pi WoL at due+960, retry+990; independently armed RTC fallback at due+1200; probe fallback+90, snapshot+180, dated review fallback+600. Latest entry22:20; all operations and review finish before23:00. Exact times and a dated power-graph request must be sent before arming. The new protocol distinguishes WoL-window, RTC-fallback, early/late/unknown returns using valid clocks and wake evidence. It makes no S3PASS or physical-duration claim from schedule alone.

This checks whether WoL fails while the independent RTC route can recover the same s2idle path. It is not proof that either wake route is already fixed. Unsupported RTC fallback, changed boot, health or window failure means leave awake and report. One transition only, safe cleanup only after inactive/unqueued sleep, no removal of wake coverage without proven cancellation, no extra wake/reset/automatic deep retry. The owner received the findings and schedule through the existing sanitized Telegram notifier.

The previously deferred12:33 review and this recovery archive remain local. Fresh21:19 remote status/flake.lock and four preimages matched the expected files; remote main.rs differed only by this session's reviewed readiness change. However, staging subsequently failed on DNS resolution, one bounded local resolution check timed out, and a single explicitly logged numeric-address attempt with the existing strict host-key alias also timed out. Neither staging SSH command reached execution. No source or documentation synchronization completed; the already imported and live-tested helper remains independently rooted on Predator.

Pending synchronization evidence is `.scratch/pm-s2idle-recovery-sync-20261031-2058/`. Its attempted payload/final hashes describe the earlier prepared snapshot; `intended-current.sha256` describes the current local documents. Rebuild a reviewed payload and inspect fresh remote status/flake.lock/preimages before an eventual write. This transport failure does not prove a new hang, boot change or failed clock repair. The21:45 workflow must freshly establish connectivity and all health/wake conditions or leave the host untouched. Existing evidence and unrelated work are preserved; no commits or pushes.
