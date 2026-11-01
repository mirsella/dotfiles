# October 31, 22:45 review: returned in the RTC-fallback window

Predator completed one s2idle cycle on boot `f9b45aa6-b820-4ab5-b18c-9324d8d53087`. The independent review verified all 20 original artifacts against hashes collected over authenticated SSH and their root:0600 metadata. Request, kernel/service return, evidence capture and cleanup succeeded. Fresh health passed at 22:46, with final read-only security/history checks at 22:48:26 to 22:48:27 CET.

Result: `RTC_FALLBACK_WINDOW_RETURN`. RTC-alarm wake is strongly supported by the combined evidence below. The exact ACPI fixed-event status bit was not captured at wake. Both scheduled Pi WoL sends ran without a recorded return in their window; the evidence does not establish why NIC wake failed.

The dated October 31 power graph covering 22:13 to 22:40 is still missing. Physical low-power duration and full PASS remain unresolved. This s2idle result is not an S3 PASS or an ordinary primer.

## Original artifacts and independent Pi evidence

The review read its schedule and deleted exact task `scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-wol-rtc-review-20261031-2245` before the 22:45:21 date guard. The pinned operator ran `collect` exactly once; both collections exited 0. The review then retrieved the originals as a separate tar stream, both frozen Pi logs, and fresh machine health. It performed no suspend, reboot, extra wake send, restoration, alarm write or timer arming.

`originals.tar.gz` preserves original root:0600 ownership/modes. Extracted local copies have local ownership; `original-tar-metadata.txt` and authenticated `final-state.txt` retain source metadata. The independent Rust verifier imports no runtime code and only reads local evidence. Its result is `independent-verification.json`; `verifier/` contains reproducible source and lockfile. Run it with the review directory as its sole argument. Its PASS covers the stated evidence assertions, not physical sleep.

Manifest SHA remains `767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942`. Original `request-call` records `SuspendWithFlags(1)` with inhibitors enforced and s2idle selected; `request-ok` is true. The observed JSON and independent after-kernel snapshot agree on exactly one entry(s2idle), idle loop, loop return and exit. They contain no deep entry, checkpoint, ACPI low-level return, BUG, Oops or Call Trace. Original operation/evidence/cleanup are all Ok; completion was recorded at 22:35:20.

Pi boot remains `2142644e-84db-40ae-b1b9-9e8c71265cf7`. Its independent baseline SHA is `fd60ed790026c2f21c602a04069d5710d4185c0c26d114b0e800642a732ce363`; result SHA is `c3d0833a9917ee17c95e6d228e6c0c2ae0569eb0d3a2ea8939d4fc770a1c7226`. The entire baseline is a byte-for-byte prefix of the frozen result. The baseline contains nonce `wake-20261031-1793481300`; the appended trial has one WAKE BEGIN and one WAKE RETURN. Netconsole stops at NIC suspend and resumes with the 22:35:20 return marker, so it does not independently record the intervening kernel phases.

The Pi journal records WoL sends at 22:31:00.620782 and 22:31:30.653041 to `98:29:a6:3a:a7:50` via `192.168.1.255:9`. Its bounded SSH probe succeeded at 22:36:30.807734; the frozen snapshot completed at 22:38:00.583988. Waker SHA remains `ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`. At 22:46:04 the receiver was active, NTP synchronized, offset +2.715ms, with no queued jobs or remaining trial timers. A successful send is not proof that the sleeping NIC received or acted on the packet.

## Wake evidence and valid clocks

At 22:15:10, `alarm-armed` records realtime and RTC epoch 1793481310, exact absolute alarm 1793482500, October 31 21:35:00 UTC alarm registers and `alarm_IRQ: yes`. This readback preceded the logind request at 22:15:11.027308. Trace stayed off and the RTC phase recorder stayed N with zero markers.

At return, before the helper corrected anything, `raw-return` records realtime and RTC epoch 1793482504, October 31 21:35:04 UTC, an empty alarm, `alarm_IRQ: no`, and wake IRQ9. Adjtimex status8193 has STA_UNSYNC clear. RTC registers still held the intended 21:35:00 alarm. The kernel recorded, in source order:

1. Timekeeping suspended for 1136.647 seconds.
2. `PM: Triggering wakeup from IRQ 9`.
3. `ACPI: PM: ACPI fixed event wakeup`.
4. `PM: resume from suspend-to-idle`.

The matching Linux6.18.58 `acpi_s2idle_wake()` checks whether any enabled fixed-event status bit is set before logging that message. It does not print which bit. This differs from the earlier trial's ACPI Notify/lid/key return. The independently read logind journal from 22:14 through 22:40 contains the request and completed operation, with no lid or power-key event.

Only the RTC wakeup-source row changed between alarm-armed and raw-return: `PNP0B00:00` active_count10 to12, event_count18 to22, wakeup_count0 throughout. NIC `0000:03:00.1`, lid and power-button rows remained zero. These are supporting counters, not a unique wake-cause decoder; RTC reads can contribute activity. The 22:48 snapshot identifies IRQ9 as ACPI and shows cumulative `ff_rt_clk=12`, `ff_pwr_btn=0`, sleep-button event invalid. There is no pre-entry fixed-event-counter baseline, so 12 is not a per-trial delta. Taken with the verified alarm and pre-correction return time, the evidence strongly supports RTC fallback without proving the exact latched bit.

Fresh NTP arrived at 22:35:07.465276. Restoration recorded valid UTC RTC at 22:35:08.503782, ordinary deep selection and unchanged btime1793474520. Correct clocks were already present in raw-return; the helper did not conceal the earlier RTC-phase corruption seen in the old trial.

## Timestamp domains and entry delay

`phase-timestamps.tsv` and the verifier JSON retain all four timestamp domains. Values below are microseconds exactly as recorded:

| Kernel phase | Source monotonic | Source boottime | Receipt monotonic | Receipt realtime |
| --- | ---: | ---: | ---: | ---: |
| entry(s2idle) | 6790650591 | 6790650591 | 6790698044 | 1793481311158132 |
| idle loop | 6842844799 | 6842844799 | 6846635802 | 1793482503743137 |
| loop return | 7979482880 | 7979482880 | 6846635923 | 1793482503743257 |
| exit | 7983055184 | 7983055184 | 6846784569 | 1793482503891904 |

Both source fields span1136.638081seconds across the idle loop. Receipt monotonic spans121microseconds and receipt realtime120microseconds because journald ingested buffered kernel messages together after return. The two equal source fields are not independent physical measurements. Their relation to receipt monotonic changes across suspend; no subtraction mixes these domains.

Systemd's returned message was received at22:35:03.574766, Finished System Suspend at22:35:03.599270, and logind completion at22:35:03.610654. Buffered kernel return/exit messages reached the journal after those service messages. Kernel source order and service return/finish order are each valid; receipt order must not be used to claim that systemd finished before the kernel actually returned.

Kernel entry to idle-loop source interval is52.194208seconds. `intel_pch_thermal 0000:00:14.2` took50.450394seconds in `pci_pm_suspend_noirq`, returning0; the complete noirq suspend phase took50.726345seconds. This accounts for most of the entry delay. The driver timing identifies a delay, not a demonstrated cause of failed WoL or earlier S3 hangs. Genuine s2idle timekeeping suspension is allowed by the cpuidle/tick_freeze path. Neither that message nor the loop interval supplies the missing dated power trace.

## Final health, preserved history and handback

At22:46 Predator was running, with no jobs/failed units/builders/active maintenance. Counters were1/0, mem_sleep `s2idle [deep]`, pm_testnone, async1, trace0, recorderN/markers0, and an empty RTC alarm. AC was online, lid closed, framebuffer blank4; LAN was1Gb/s with WoLg, declared UUID`c4e6863b-7f76-4a4d-9a48-9937af0b5a36`, DNS192.168.1.1 and synchronized NTP/UTC RTC. Both pools were ONLINE with no known data errors, all five dataset mounts were correct, all listed application services were active and the four Immich containers healthy. The Nextcloud cron timer last fired22:46:03 and was next due22:51:03.

All five authenticated boot and late-phase rejection audits passed. Frozen system/kernel/modules and runtime/helper hashes matched. Final checks confirmed TPM CRB, device nodes, Acer module exclusions, no raw-EC service journal entries and no HOLD in any study state directory. Historical completion, calibration, earlier S3 manifest, old s2idle manifest/raw-return and current boot-RTC hashes matched their pins. The old ae29 boot retains its returned s2idle followed by orderly poweroff; f8eb retains ten diagnostics and one owner-confirmed15W failed S3. Missing STATE/PLAN history remains unresolved. Old runner source hashes and immutable preparation checksums still pass.

At22:48 both systemd-suspend and the primary runner were inactive/MainPID0, Resultsuccess/ExecMainStatus0, with no queued sleep job. The pinned runner had already finished cleanup; the review invoked no restore and removed no alarm or runtime root. No further power operation is scheduled. The owner should supply the dated22:13 to22:40 graph before physical-duration classification.

The intended synchronization is limited to this review archive and the three study index/protocol READMEs. Fresh remote Git status and flake.lock inspection are in `final-state.txt`; the remote lock had no diff. The separate guarded-sync directory records preimages, payload checksums and the actual installation receipt. Existing preparation evidence, unrelated work and all GCroots are preserved. No commit, push, system activation or frozen configuration change was performed.
