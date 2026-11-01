# Serial Q37 preparation, October 30, 2026

Status: **STARTUP BLOCKED, awaiting owner recovery; no suspend armed**. The owner authorized automatic continuation after Q35r1's maintenance refusal. Healthy-machine shutdown was acknowledged and the single planned WoL ran, but the 15:10 follow-up still found no reachable new boot. Its task has been deleted and no replacement is scheduled. This is not a Q37 suspend result. New boot ID, btime, manifest and exact suspend/wake deadlines remain unset.

## Selected experiment and standing authorization

Owner instruction: **"automatically do that, always schedule the next test"**. The selected next test is first-cycle serial **Q37**, due exactly **btime + 2220 seconds**, wake due plus 960, review wake plus 600 rounded up. This explicitly changes the short-age target from 35 to 37 minutes within Q's 1800..2400-second range to avoid the observed five-minute Nextcloud cron phase. Inspect the actual timer forecast before arming and retain the maintenance guard. Do not silently shorten or substitute an age, or describe Q37 as an exact Q35 replication.

Continue automatically after eligible reviews with one justified next test at a time, carrying the standing policy forward. Healthy controlled startup is authorized when needed for a first-cycle test. Preserve the observation window, physical evidence, timing, ordinal, baseline and owner-only hung-machine recovery rules. Missing physical evidence cannot produce a compliant passing bound or primer. Independent replication is possible without promoting the preceding result. A real startup/health blocker must be reported rather than bypassed with remote recovery attempts or unconditional repeated resets.

Required unchanged baseline:

- System `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`.
- Kernel `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`.
- Modules `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
- Corrected runtime package `/nix/store/5gqrh1r71xnl127sj8dv83nnlk1nf6vs-predator-pm-study-0.1.0`, with the existing 24 passing tests/build evidence in Q35r1's preparation archive. No code or deployment change during this preparation.
- Runtime binary SHA256 `6681936bb69ab828f41d7207282cd1cf95f1a4e155804a4b266bcb15a1805bea`; controller `56857827226a4e55319ffa69dec8ad269c3ed7ed7ff6a021f41915535901246c`.
- Schema2, ordinal1, pm_async0, trace0, recorder off, deep, pm_test none, initialized TPM, raw EC not-run, users allowed with inhibitors enforced. Ordinary async1 and zero prior cycles/markers are required before execution. Keep old system and new runtime GC roots.

Prior Q35r1 never suspended because its final preflight found Nextcloud cron activating. Prior Q35 resumed for954.773 seconds with owner-observed3W but failed the old verifier and lacks a dated graph. Prior long-age asyncD/serialD are owner-confirmed approximately15W hangs. These are separate observations; no compliant reproducible age bracket exists and Q cannot prime E.

## Healthy shutdown and planned startup

At **14:52:30 CET**, old boot `3c1873e4-b866-41e1-b411-702168ef90cf` passed baseline/runtime, all five authenticated disk/late-phase audits, health/NTP/storage/application checks. Counters0/0, recorderN/markers0, ordinaryasync1, trace0, AC1; no jobs, HOLD, active checked maintenance or nixbld processes. Only the known Q35r1 refusal unit was failed. Pi receiver/NTP/waker checks passed, with no future real-trial timers.

One `systemctl poweroff --no-block` was acknowledged at epoch **1793368405**, **14:53:25 CET**. Pi startup unit `predator-pm-q37-preparation-20261030-startup` was nonpersistent, accuracy1s/random0, timeout30s, guarded by root-only `/run/predator-pm-q37-preparation-20261030-shutdown-ack`. The guard was created only after successful acknowledgement. Scheduled startup epoch **1793368584**, **14:56:24**; actual single WoL send **14:56:24.646608**, successful service deactivation **14:56:24.661654**. A bounded 250-second read-only journal follower expired and was reaped; independent journal output confirms the send. The spent guard was removed after the service was inactive.

At **14:58:34**, the first authenticated post-boot inspection failed at SSH transport with `No route to host`; its remote health script did not run. At **14:58:59**, the Pi's bounded LAN keyscan also failed. The receiver remained active. Its live log last changed **14:53:34.742530279**, during shutdown callbacks, with last captured line `[4541.323329] r8169 0000:03:00.1: shutdown`. There are no captured new-boot messages. Neither that boundary nor the absence of SSH establishes physical off-state, a driver culprit or the failed phase. New boot identity/btime, shutdown completion and physical off duration are not independently established. No additional wake, reset or security experiment was attempted.

Frozen Pi file `/home/mirsella/dev/predator-pm-debug/.agents-local/r-serial-q37-20261030-startup-blocked.log` was created at **14:59:48**. Complete local copy: **2,181,931 bytes**, SHA256 `78dc2e2b53368e389aebdc48a9960782bd65962b47c8ee7c9cb9db31bf49b385`. Telegram notice **88801**, epoch1793368929, requested owner watts/LEDs and local recovery/startup as needed. Physical state remains unknown.

## Preparation check completed at 15:10

Exact task: `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q37-preparation-20261030-1510`.

Previously enabled with verified next run **`2026-10-30T14:10:00.000Z`**, **15:10 CET October30**. It was successfully deleted at follow-up start; the dated guard passed at **15:10:19 CET**. This was one guarded preparation check, not an armed suspend or a repeating poll.

The single bounded authenticated health inspection ran from **15:10:43 to 15:10:46** and failed at SSH transport, exit255, `No route to host`. Its stdout is empty: the copied read-only health script did not execute. Current Predator boot ID, btime, health, counters, maintenance forecast and old-boot shutdown journal remain unavailable. Do not treat unavailable retrieval as proof that remote artifacts or a boot do not exist.

Pi inspection at **15:11:00 to 15:11:03** found boot `2142644e-84db-40ae-b1b9-9e8c71265cf7`, receiver active, NTP synchronized (packet559, offset+2.816ms) and the expected waker hash. Bounded LAN keyscan exited1; Predator's neighbor state was `FAILED` before and after it. Only expired historical notice timers remained, with no future real-trial timer; the spent startup acknowledgement guard was absent.

The live netconsole log still had last modification **14:53:34.742530279**. A create-new frozen file, `/home/mirsella/dev/predator-pm-debug/.agents-local/r-serial-q37-20261030-followup-1510.log`, was saved at **15:11:03.390389606** and downloaded in full. Its **2,181,931 bytes** and SHA256 `78dc2e2b53368e389aebdc48a9960782bd65962b47c8ee7c9cb9db31bf49b385` match the earlier startup capture byte-for-byte. There are no newly received boot messages. The final shutdown callback still cannot establish physical state or a driver cause.

Classification: **STARTUP_BLOCKED_AWAITING_OWNER_RECOVERY**, not a suspend result. Telegram **88803**, epoch1793369486, reported the blocker and requested current watts/LEDs/screen and local recovery/startup as needed. No extra wake, reset, runtime change, manifest, suspend or replacement polling task was created. Automatic continuation remains authorized once a healthy eligible boot is available; this unresolved startup cannot be bypassed to invent a test deadline.

The owner then asked whether Predator was just building heavily. One read-only check at **15:17:13 CET** again failed SSH with `No route to host` before load/process inspection could run. At **15:17:18**, the Pi found neighbor state `FAILED`, then `INCOMPLETE` after two unanswered pings. Its active netconsole receiver still held the identical 2,181,931-byte log, last modified at 14:53:34. No Nix builders were present at the earlier healthy pre-reset inspection; subsequent captured messages were kernel device shutdown. These observations make ordinary build load less likely, but do not establish the current power state or rule out a severe stall/network failure. No live process/load reading was obtained and no power action was taken. Captures are in `load-check-20261030-1517/`.

If a new healthy boot is reachable, establish its exact identity/btime and observed startup provenance, recover the old compact-UUID shutdown journal, perform full health/USB/timer checks, and create a fresh immutable manifest only when `btime+2220` still has more than300seconds arming lead. All suspend/wake/review must fit08:00..23:00 and the manifest's at-most12hour subset. Use the corrected pinned controller stage/rehearse/arm; verify observer coverage and schedule the dated self-deleting review. Propagate automatic continuation into that review. If startup is still blocked, preserve evidence and stop for owner recovery; do not send another WoL or fabricate a test deadline. An aged-out boot cannot be relabeled Q.

Every actual test still needs requestOk, exact boot/baseline, one ordered current-boot deep entry/resume/exit, counters1/0, markers0, current measured sleep>=900seconds, valid matching completion, fresh clock, both pools/five mounts, and its own dated low-power graph for fullPASS. The corrected verifier's raw early snapshot and final verification history must be preserved separately; no completion synthesis.

## Evidence and synchronization

Complete pre-reset checks, startup unit/proofs, failed transport/LAN probes, full frozen Pi capture, owner instruction/notice and preparation schedule are archived with checksums under `preparation-blocked-20261030-1451/`. The dated follow-up's health script, transport failure/status/timestamps, Pi state and complete snapshot, notification acknowledgement and pre-edit README copies are preserved under `followup-blocked-20261030-1510/`. Private notification credentials are excluded. Task scratch follows the updated environment policy without overriding tool-managed `TMPDIR`. No trial manifest exists yet. New successful preparation must use a new directory and preserve both archives.

Remote synchronization is pending while Predator is unreachable. At14:52 remote main README SHA256 was `3889118e0b42971fe6fc3c4f88619c2b9a7fb405575d6c73dda421287dc741b3`; priorQ35r1 README `bc0b61b978c01ebfb42dd1ca283264b4de01920c8356a7c52be24a7b6aab7f71`. Recheck before replacing only intended documentation/new evidence paths. Runtime source hash remains `dc626ff76f258e1dd36f5f0bef4285ee0951482b135dccccf0333ca8bb779d86`. Unrelated changes and old captures remain intact. Missing original STATE.md/PLAN.md are not reconstructed.
