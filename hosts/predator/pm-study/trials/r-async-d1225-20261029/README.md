# Fresh async D on October 29, 2026

Status after the owner's October 29 clarification: **owner-confirmed HANG**. The owner reported it stuck at about 15 W and used a power-button hold to recover it; see `owner-report-20261029.md`. Deep-suspend sequence entry was captured, but Predator did not become reachable after either scheduled WoL send. Recovery collection found no return/result/completed files. The healthy new boot is recovery, not a successful D resume. The dated power graph and measured sleep duration remain unavailable.

Arming had succeeded October 28 around 21:02 CET. At 21:04 the primary and all observer timers were waiting, the primary service was inactive, suspend counters were 0/0 and recorder was N.

- Boot: `b223cb35-a02b-4417-a3db-569ad986cd4b`, btime `1793217362`, October 28 at 20:56:02 CET.
- Profile D, first suspend, ordinal 1, target age 44,700 seconds, `pm_async=1`.
- Request: October 29 at **09:21:02 CET**, epoch `1793262062`.
- Wake: **09:37:02**, epoch `1793263022`; retry 09:37:32, probe 09:38:32, snapshot 09:40:02.
- Evidence review: **09:48 CET**, after the wake-plus-600-second threshold of 09:47:02.
- Manifest: `preparation/plan.conf`, SHA256 `a2fb0df21a5e800da11d4e80763bc7d18686fab9fa78278f8144c4c68b63f1da`.
- System: `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`.
- Kernel: `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`.
- Modules: `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
- Runtime: `/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study`.

TPM initialized, raw EC not-run, recorder off, trace 0, deep sleep, pm_test none. Login policy is `allow`; logind inhibitors remain enforced. The owner-approved observation window is 08:00 to 23:00 Europe/Paris. This morning manifest uses 08:00 to 20:00 to satisfy the deployed parser's 12-hour maximum.

## Reset context

The agent requested one ordinary reboot at 19:36:29 on October 28 after healthy idle checks. The old journal records the reboot target; netconsole ends at NIC shutdown. The owner subsequently reported finding Predator powered off and sending WoL, after which this new boot appeared. The agent did not perform recovery WoL, a power toggle or another reboot. The cause of the powered-off state is unverified.

This is a first-suspend baseline following an owner-reported powered-off state and owner WoL, not the intended uninterrupted warm reboot. The physical off-state, off duration and WoL transmission time were not independently measured. Preserve this distinction when planning serial D; matching kernel and age alone does not match reset history.

## Preparation evidence

`preparation/` contains actual boot and prior shutdown journals, owner/reset context, disk and USB topology, readiness, staged manifest, harmless timer rehearsal, complete controller arm captures, observer baseline, notification status and final timer proofs. The missing `lsusb` command ended one read-only collection with status 127 after both journals and `lsblk`; USB sysfs topology, sessions and inhibitors were then captured separately. No package or baseline was changed to obtain that information.

All five authenticated disk boot audits and late-phase rejection checks passed. Current/booted identity, TPM, services/containers, pools/mounts, fresh NTP and study preflight passed. Telegram setup message 88727 was acknowledged by the API. Credentials were sent through SSH stdin to a root-only Pi config and are not captured in this archive.

The review task was `scheduled-task:command:mcp:c74dd2c1-2c4d-40bb-bc82-59208cb578e3:schedule-task:predator-async-d1225-review-20261029-0948`, with nextRunAt verified as `2026-10-29T08:48:00.000Z`. The dated review ran at 09:48:30 CET and deleted this exact task. It collected without sending extra recovery WoL or resetting the machine.

A full PASS requires resultOk, the expected boot, one ordered deep cycle, counters 1/0, markers 0, current-cycle measured sleep at least 900 seconds, fresh NTP, healthy storage and the owner's dated low-power graph. The 960-second scheduled wake delay is not a sleep-duration measurement. `SHORT_SLEEP` is a deviation. The old H hang remains a separate result on the earlier kernel.

## October 29 outcome evidence

`review-20261029-0948/` preserves the controller's partial collection, complete frozen Pi log and SHA256SUMS, notification journals and live-Pi status. Predator SSH failed with exit 255, `No route to host`; no remote collection commands ran. The empty Predator output is an access gap, not evidence that the runtime created no artifacts.

Boot-bound netconsole records:

- Preflight at 44701 and 44704 seconds awake, prior successes 0 and markers 0; all three disk warmups recorded.
- BEGIN at monotonic 44704.334011; suspend request at 44704.418006.
- `PM: suspend entry (deep)` at 44704.582941, received at 09:21:06 CET.
- Last record: `leds input0::scrolllock: PM: led_suspend [led_class] returned 0 after 0 usecs` at 44704.689691, received at 09:21:06. This is a logging boundary, not a culprit or proof of physical sleep.
- No subsequent suspend exit, resume, verified record or timekeeping sleep duration for this boot.

The Pi sent scheduled WoL at 09:37:02.651632 and the retry at 09:37:32.655601. Both commands completed successfully; packet receipt and resume are unproven. The LAN SSH probe failed at 09:38:35 with exit 1, and the snapshot completed at 09:40:02. Successful transient units had been garbage-collected by review time, leaving empty service timestamps and missing-unit messages; the retained journals establish their actual execution.

The snapshot is 1,547,549 bytes, SHA256 `9676887af405220bac4726f50c47916bb9003594f0ec64d6805dc601429fea37`, verified against the Pi copy. At 09:50:34 the receiver was active, NTP synchronized, waker hash unchanged and neighbor `FAILED`; the live log matched the frozen log and had not changed since 09:21:06.915515853. The file contains older boots, whose events cannot establish this trial's return.

Telegram notices were acknowledged: start 88736, first wake 88737, retry 88738, failed probe 88739 and frozen snapshot 88740. Review notice 88741 requested current physical status and the dated power graph. Neither owner confirmation nor the graph was available during this review.

At the morning review, no resultOk, return/completion artifact, measured sleep duration, post-cycle counters, current boot identity, fresh Predator NTP or post-return storage health could be collected. No extra recovery WoL, reboot, power toggle or security change was attempted. No new trial or replacement review was scheduled. Retained artifacts and boot-specific journals were subsequently collected after owner recovery, as follows.

## Recovered evidence on October 29

The owner first reported "ive rebooted it". At 13:20:27 CET, SSH reached new boot `4e5cb3ba-062b-4ad0-bd18-7e24492f97e6`, btime `1793271910` = October 29 at 12:05:10 CET. Current and booted generation, kernel and modules match the pinned baseline. No intermediate retained boot is listed. The owner later confirmed a stuck state at about 15 W and power-button-hold recovery. The startup method after the forced shutdown, exact timings and dated graph were not supplied. Earlier captures correctly record that these details were unknown at collection time.

`recovered-20261029-1320/` preserves the successful second controller collection (`issues=[]`), exact retained files and separate old-boot kernel journal, current health checks and checksums. The earlier partial collection is unchanged. The frozen Pi result still hashes to `9676887af405220bac4726f50c47916bb9003594f0ec64d6805dc601429fea37`.

- Retained plan and requested files: SHA256 `a2fb0df21a5e800da11d4e80763bc7d18686fab9fa78278f8144c4c68b63f1da` each.
- Retained before capture: 111,058 bytes, SHA256 `ea4e3b82d0fe520732808af8eb29f3b8473cb877de91c59b67d0a2c74f9b9808`.
- No return, result or completed file for this trial exists in the inspected state directory.
- Old service journal: user.slice froze, then systemd-sleep logged "Performing sleep operation 'suspend'..." at 09:21:06.757404 CET. No subsequent service entry was retained for this boot.
- Persisted old kernel journal ends at the suspend request. It does not retain the later callback records present in Pi netconsole. Neither source records a return or measured sleep duration.

The recovered boot has counters 0/0, recorder N and no PM-cycle markers, no HOLD, no failed units/jobs or active maintenance, healthy services/four Immich containers, healthy pools/five mounts and fresh NTP. All five authenticated boot and late-phase rejection audits passed. The declared LAN/DNS, initialized TPM, absent Acer WMI modules, raw EC not-run, AC/lid/screen/deep/WoL settings and pinned clock-handoff are correct. These are new-boot health checks, not evidence of a successful previous cycle.

Its first-suspend D deadline would be October 30 at 00:30:10 CET, outside the owner's window. No trial was armed or additional reboot performed. Serial D requires a fresh in-window start and a documented reset comparison with this trial's owner-reported powered-off then WoL baseline.
