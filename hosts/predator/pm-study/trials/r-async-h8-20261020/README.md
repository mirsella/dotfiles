# r-async-h8-20261020: owner-confirmed hang

Testing is paused at the owner's request on October 21. All study schedules have been removed. Wait for the owner to report that someone has booted Predator and request resumption; do not automatically probe, recover, prepare or arm another trial.

Review at 20:51 to 20:54 CEST on October 20 established an executed suspend attempt, not a canceled timer or preflight refusal. The owner subsequently confirmed a hang and said nobody was home to reboot it, recorded at 23:26. Detailed wattage, the full dated graph and the exact failure phase remain unavailable. No compliant PASS can be claimed.

The read-only 21:15 follow-up found Predator still unreachable, with no new Pi messages since 20:24:15. No owner power reading, graph or attended-recovery confirmation had been supplied at that time; see `followup-2115/STATUS.md`. The subsequent owner report is preserved in `owner-report-2326.md` without modifying earlier captures.

The October 21 morning check at 09:31 again found Predator unreachable. Pi's receiver was active, but its live log was unchanged since October 20 at 20:24:15; the complete frozen snapshot retained its verified hash. No subsequent owner recovery or plug-operation report was available. See `followup-20261021-0930/STATUS.md`. The owner's HANG verdict stands; no recovery, new suspend or preparation was attempted.

The October 21 afternoon check at 17:01 was unchanged. Predator's explicit WAN SSH route returned `No route to host`, exit 255, before any remote command ran. Pi's receiver was active, its neighbor entry was FAILED, and its live log was still 1,201,689 bytes with modification time October 20 at 20:24:15. The frozen snapshot's complete SHA256 still matched the archived copy. There were no future Pi study timers, only elapsed notices. No new owner recovery, plug action, numeric wattage or graph was available. Fresh raw captures and checksums are in `followup-20261021-1700/`; retained scratch copies remain under the dotfiles checkout. No recovery or new trial was attempted.

## Verified trial identity

- First suspend, ordinal 1, async 1, H target 8 hours 5 minutes awake.
- Boot `3e3c970d-025f-4b78-8e00-f911e202e948`, epoch `1792491551`, October 20 at 12:19:11 CEST.
- Booted and current generation matched `/nix/store/lzw3aij1nckap2iwdylhybla25kaz4nz-nixos-system-predator-26.11.20261001.c59305b` at the verifier and entry preflight.
- Kernel `/nix/store/b1slgijwvdxlci37m675ai43by6nf8ak-linux-6.18.54/bzImage`; modules `/nix/store/6qf4yh1w3y4ams8hsa83m3s1bafqr4r5-linux-6.18.54-modules`.
- Runtime/controller package `/nix/store/5qjb1am045yn7wxqgz24cr919amdza28-predator-pm-study-0.1.0`.
- Exact recovered manifest SHA256 `ef84699fe4007f6c9a749a045a2dabd8ecf015f48182e5c44164802c465fcd4e`.
- Due 20:24:11, wake 20:40:11, retry 20:40:41, probe 20:41:41, snapshot 20:43:11 CEST.
- Pre-entry state: counters 0/0, phase markers 0, trace 0, recorder off, deep selected, pm_test none, TPM initialized, raw EC not run. These are pre-entry observations, not post-return health results.

## Observed execution

The frozen Pi capture records entry preflight at awake ages 29100 and 29103 seconds, the exact BEGIN trial identity, a suspend request at kernel time 29103.475646, and `PM: suspend entry (deep)` at 29103.639790. The Pi received these at about 20:24:15.

The final visible message is `[29103.718187] i2c_designware i2c_designware.1: PM: calling platform_pm_suspend @ 198772, parent: 0000:00:15.1`. It is an observation boundary, not proof that this driver caused a hang. Async callbacks overlap, and netconsole can stop when networking suspends.

Pi journals confirm both scheduled WoL sends succeeded at 20:40:11 and 20:40:41. That proves transmission, not receipt by Predator. The scheduled LAN SSH probe failed at 20:41:44. A fresh read-only WAN SSH check at the review returned `No route to host`; the Pi neighbor was FAILED and its additional read-only LAN probe returned no host key.

There is no resume, measured sleep duration or verified completion for this trial in the frozen Pi excerpt. Predator's own result, kernel journal, counters, NTP and storage recovery state remain unavailable. Network loss alone does not distinguish failed entry, failed wake or failed return.

## Complete observer evidence

The scheduled 20:43:11 snapshot succeeded on Pi at `/home/mirsella/dev/predator-pm-debug/.agents-local/r-async-h8-20261020-result.log`. Its complete byte-exact local copy is `review-2051/netconsole-result.log`:

- Size 1,201,689 bytes.
- SHA256 `d2c1ba165c52d45efaf9d7beb8e0583a5d1d09b722d7508bfc52c35a7b5eb8ed`.
- Downloaded in 37 validated chunks and verified against the Pi hash. The frozen Pi source was not changed.
- `review-2051/trial-entry-excerpt.txt` is a derived current-trial excerpt. The full source contains older boots too; do not mistake their resumes for this trial.
- `review-2051/observer-detail.txt` retains the full trial and notification service journal. Start, wake and retry Telegram notices failed with API connection timeouts, but the actual WoL services succeeded. Probe-failure and snapshot notices were delivered as messages 88671 and 88672.
- `verify-1924/` retains the preceding verifier and the exact recovered manifest. The unavailable original local captures have not been recreated.

This is a manual partial collection, not a complete successful controller collection. `review-2051/collection.txt` lists the missing evidence. Checksums establish local file integrity, not a successful suspend outcome.

## Classification and next action

Current status is HANG, confirmed by the owner. This does not establish whether entry, wake or return failed. Exact wattage and the full dated graph covering baseline, entry, sleep interval and scheduled wake remain missing. Do not invent power measurements or treat the last callback as the cause.

At 23:26 on October 20, the owner asked whether draining the battery by switching off AC, restoring AC and sending WoL could recover it. That message was a question, not a reported power action or authorization for agent recovery. On October 21 the owner subsequently reported that the battery reached 0% and WoL did not boot Predator, then requested pausing the schedules until someone boots it. The owner's explanation that wake configuration was lost has not been verified by inspecting firmware or the NIC. No agent plug operation, extra WoL or other recovery command was performed.

There is no scheduled follow-up while the study is paused. No agent restart, extra WoL recovery, power toggle, unlock, TPM/LUKS/keyslot/firmware change, new suspend or new preparation reboot was performed. Booting Predator is the owner's action. After the owner reports recovery and requests resumption, preserve any retained old-boot artifacts before another preparation.

The spent 20:51 review, 21:15 follow-up and October 21 09:30 and 17:00 checks were deleted. The remaining October 22 `pm H recovery Thu` task was also deleted at the owner's request. The scheduler was checked afterward and contained no tasks bound to this thread. No polling, preparation or real trial is scheduled.

An additional Pi review notification was attempted using its existing private curl configuration, without reading or displaying credentials. It failed with Telegram API connection timeout, curl exit 28. This is recorded in `review-2051/review-notice.*`; delivery must not be claimed. The earlier probe-failure and snapshot messages did succeed.

When the owner requests resumption after recovery, first collect retained H artifacts and establish actual boot identity and health. The unfinished sequence is fresh async D, then matched fresh serial D and controlled repeat E with identical serial D primers. Do not silently substitute ages, reboot an armed or stuck machine, or discard an eligible recovered boot. New preparation requires working access/storage, fresh health gates and the full 09:00 to 21:30 observation window.
