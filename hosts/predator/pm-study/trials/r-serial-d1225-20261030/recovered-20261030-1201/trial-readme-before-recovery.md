# Fresh serial D, October 30, 2026

Status: **suspected hang, awaiting owner confirmation**. The first suspend on boot `13bec0dd-b611-43aa-a9b7-30a8de33195e` began at 08:30:45 CET after about 44,705 seconds awake, with `pm_async=0`. Both scheduled wake sends ran, but the LAN probe and review SSH failed. No verified completion is available, so repeat E is not armed.

## Identity and schedule

- Boot time: `1793300740`, October 29 at **20:05:40 CET**.
- System: `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`.
- Kernel: `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`.
- Modules: `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
- Runtime: `/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study`.
- Manifest: `preparation/plan.conf`, SHA256 `971f5fd85e83510230a42372ad5a841c28e87a4fceddca26b2c7c4b8a5ba2594`.
- Request: `1793345440`, October 30 **08:30:40 CET**.
- First WoL: `1793346400`, **08:46:40**; retry **08:47:10**.
- LAN probe: **08:48:10**; frozen Pi result: **08:49:40**.
- Review: **08:57**, rounded up from wake plus 600 seconds, 08:56:40.

Schema 2, profile D, ordinal 1, trace 0, recorder off, deep, pm_test none, initialized TPM, raw EC not-run, logged-in users allowed with logind inhibitors enforced. Manifest window 08:00–20:00 is a valid subset of the owner's 08:00–23:00 Europe/Paris window. The runner temporarily selects serial mode and restores ordinary `pm_async=1`.

Review task: `scheduled-task:command:mcp:ec3a1d16-3127-4eb3-a89b-dcdd45951c21:schedule-task:predator-serial-d1225-review-20261030-0857`. Its verified run time was `2026-10-30T07:57:00.000Z`. The review ran within its October 30 guard and confirmed exact task deletion after schema discovery resolved an initially unavailable tool.

## October 30 evidence review

The pinned collector retrieved the full frozen Pi capture and observer journals at 09:00 CET. Predator SSH exited 255 with `No route to host` before executing the collection script. The result is partial; retained Predator artifacts, current boot identity, post-cycle counters, NTP and storage state remain unavailable. Their absence on disk has not been established.

The current attempt's capture records:

- Preflight on the expected boot/generation with prior successes 0 and markers 0.
- BEGIN at monotonic **44704.785003**, ordinal 1, `pm_async=0`, trace 0 and recorder off.
- Suspend request at **44704.860992**, then `PM: suspend entry (deep)` at **44705.023600**, received at 08:30:45 CET.
- Last line at **44705.842762**: `r8169 0000:03:00.1: PM: calling pci_pm_suspend`, received at 08:30:46. The logging boundary does not identify the cause or establish physical sleep.
- No captured current-cycle resume, exit or timekeeping sleep duration. Older boots in the full log must not be attributed to this trial.

The Pi sent WoL at **08:46:40.650787** and **08:47:10.644423**. The LAN probe failed at **08:48:13**, and the snapshot succeeded at **08:49:40**. At 09:02 the receiver was active, Pi NTP synchronized, the waker hash matched and Predator's neighbor entry was `FAILED`. No future Pi study timer was listed. Transient-unit garbage collection explains empty execution fields; the journals record the actual sends.

Live and frozen logs both contain **1,851,536 bytes**, SHA256 `43843381f6fecbb8167d3c8f89c9e6112a098829769ba6bb6a6a0141d619cb8a`. The local full copy matches. Files under `review-20261030-0857/` retain the full captures, metadata, derived attempt excerpts, collection gaps and checksums. The last live-log write was 08:30:46.645519791.

Telegram notices were acknowledged: start 88763, wake 88764, retry 88765, failed probe 88766 and snapshot 88767. Review message **88768** requested current watts, LEDs/screen, confirmation whether it is stuck and the dated power graph. The physical state is unknown. This is not a technical PASS awaiting only a graph, and owner-confirmed HANG has not yet been established. Measured sleep and SHORT_SLEEP status cannot be assessed.

The review stopped for owner confirmation/recovery. It sent no extra WoL and requested no reset or power toggle. It armed no repeat or replacement trial and scheduled no new task. E has no verified `completed_epoch` anchor. After recovery, collect this boot's retained artifacts and journals before choosing the documented next branch. README/archive synchronization to Predator is blocked by its unreachability; the review archive is currently local.

## Startup and comparison limits

The previous recovered boot was healthy and idle, with no prior suspends, HOLD, failed units, pending jobs or active checked maintenance. Its 12h25 deadline was outside the observation window. One ordinary poweroff was accepted at **20:02:26 October 29**; the journal records the System Power Off target finishing at 20:02:35. A root-only acknowledgement condition enabled the single prearranged Pi WoL at **20:05:24.664264**. No forced shutdown or startup retry occurred. The new boot began at 20:05:40.

Physical power-off and its duration were not independently measured. The failed async D began after an unexplained powered-off state following a reboot request, then owner WoL. The new controlled shutdown/WoL start is only nominally comparable, not an exact reset match. A serial success would support further testing of async ordering, not prove a cure.

All captured USB paths, vendor/product identifiers, serials, speeds, power controls and runtime-status snapshots match the async D preparation. Exact baseline, all five authenticated boot audits and late-phase rejection, TPM/EC/Acer checks, AC/lid/screen/NIC, fresh NTP/LAN, services/four containers, pools/five mounts and zero PM counters/markers passed on the new boot.

The startup journal follower timed out because its match omitted the unit prefix in the Finished message. Independent journal inspection confirmed the single send and successful service completion. The expired read-only follower and spent acknowledgement guard were cleaned up; no extra packet was sent. Empty fields from garbage-collected transient units were not used as evidence of failure. An ad-hoc `pgrep -u nixbld` check used a nonexistent username; the full process list and correct `pgrep -G nixbld` check closed that gap, and the pinned runtime uses the group correctly.

## Verified preparation

Pinned controller stage, schema-2 preflight, harmless timer rehearsal and arm succeeded. At **20:11 October 29**, independent checks found the primary and all Pi observer timers waiting at the exact times above, the primary service inactive, counters 0/0, recorder N/markers 0 and ordinary `pm_async=1`. The nightly and legacy-next suspend timers remained inactive. Telegram setup message **88755** was acknowledged. Full captures and checksums are in `preparation/`; older captures are not overwritten.

## Review and conditional continuation

Require result Ok, one ordered deep cycle on this boot, counters 1/0, markers 0, measured current-cycle sleep of at least 900 seconds, fresh NTP, healthy storage and the dated owner low-power graph for full PASS. The 960-second wake delay is not physical sleep evidence. SHORT_SLEEP is a deviation. A new SSH connection or new boot alone is not a successful return.

Only a compliant serial D with physical evidence can prime same-boot async repeat E: ordinal 2, `pm_async=1`, due at this runtime's verified **`completed_epoch + 25500`** (7h05). Check the D completion artifact against `last-completed.txt`; do not substitute returned_epoch, boot time or review time. Require no intervening sleep/reset, full preflight, more than five minutes of arming margin, and the full cycle/review within the observation window. No E trial is armed now. If the graph is missing, request it and arrange only a justified evidence follow-up before the deadline. A later serial E comparison needs a separate fresh boot with the same successful serial D primer.

If this D hangs, collect the Pi evidence and await owner recovery. No extra recovery WoL, reboot, power toggle or security-policy experiment is authorized. After recovery, the next research branch is a short-age control in one fixed mode, then age bisection only after establishing a repeatable bracket. Preserve reset differences and avoid a threshold or cure claim from one observation.
