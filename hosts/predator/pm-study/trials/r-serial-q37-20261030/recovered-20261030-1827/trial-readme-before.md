# Serial Q37 trial, October 30, 2026

Status: **SUSPECTED HANG, physical state unknown; automatic continuation blocked pending owner recovery**. The first serial suspend entered its deep sequence at **17:44:21 CET**, but no return was received. Both planned wakes ran; the LAN probe and review SSH failed. The review task is deleted and no further test is armed. Expected boot is `614a8318-dc72-4308-ae89-d4680fafac4e`, begun **17:07:18** after the owner's recovery. The earlier approximately30 W report belongs to the separate startup outage, not this suspend. Exact owner reset method/off duration remain unspecified. Earlier preparation evidence remains immutable.

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

At **14:58:34**, the first authenticated post-boot inspection failed at SSH transport with `No route to host`; its remote health script did not run. At **14:58:59**, the Pi's bounded LAN keyscan also failed. The receiver remained active. Its live log last changed **14:53:34.742530279**, during shutdown callbacks, with last captured line `[4541.323329] r8169 0000:03:00.1: shutdown`. No new-boot messages had been captured. Neither that boundary nor the absence of SSH establishes physical off-state, a driver culprit or the failed phase. New boot identity/btime, shutdown completion and physical off duration were then unavailable. No additional wake, reset or security experiment was attempted.

Frozen Pi file `/home/mirsella/dev/predator-pm-debug/.agents-local/r-serial-q37-20261030-startup-blocked.log` was created at **14:59:48**. Complete local copy: **2,181,931 bytes**, SHA256 `78dc2e2b53368e389aebdc48a9960782bd65962b47c8ee7c9cb9db31bf49b385`. Telegram notice **88801**, epoch1793368929, requested owner watts/LEDs and local recovery/startup as needed. Physical state was unknown at that inspection.

## Preparation check completed at 15:10

Exact task: `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q37-preparation-20261030-1510`.

Previously enabled with verified next run **`2026-10-30T14:10:00.000Z`**, **15:10 CET October30**. It was successfully deleted at follow-up start; the dated guard passed at **15:10:19 CET**. This was one guarded preparation check, not an armed suspend or a repeating poll.

The single bounded authenticated health inspection ran from **15:10:43 to 15:10:46** and failed at SSH transport, exit255, `No route to host`. Its stdout is empty: the copied read-only health script did not execute. Predator boot ID, btime, health, counters, maintenance forecast and old-boot shutdown journal remained unavailable then. Do not treat unavailable retrieval as proof that remote artifacts or a boot do not exist.

Pi inspection at **15:11:00 to 15:11:03** found boot `2142644e-84db-40ae-b1b9-9e8c71265cf7`, receiver active, NTP synchronized (packet559, offset+2.816ms) and the expected waker hash. Bounded LAN keyscan exited1; Predator's neighbor state was `FAILED` before and after it. Only expired historical notice timers remained, with no future real-trial timer; the spent startup acknowledgement guard was absent.

The live netconsole log still had last modification **14:53:34.742530279**. A create-new frozen file, `/home/mirsella/dev/predator-pm-debug/.agents-local/r-serial-q37-20261030-followup-1510.log`, was saved at **15:11:03.390389606** and downloaded in full. Its **2,181,931 bytes** and SHA256 `78dc2e2b53368e389aebdc48a9960782bd65962b47c8ee7c9cb9db31bf49b385` match the earlier startup capture byte-for-byte. There are no newly received boot messages. The final shutdown callback still cannot establish physical state or a driver cause.

Classification at that follow-up: **STARTUP_BLOCKED_AWAITING_OWNER_RECOVERY**, not a suspend result. Telegram **88803**, epoch1793369486, reported the blocker and requested current watts/LEDs/screen and local recovery/startup as needed. No extra wake, reset, runtime change, manifest, suspend or replacement polling task was created. Automatic continuation remained authorized once a healthy eligible boot became available; the unresolved startup was not bypassed to invent a test deadline.

The owner then asked whether Predator was just building heavily. One read-only check at **15:17:13 CET** again failed SSH with `No route to host` before load/process inspection could run. At **15:17:18**, the Pi found neighbor state `FAILED`, then `INCOMPLETE` after two unanswered pings. Its active netconsole receiver still held the identical 2,181,931-byte log, last modified at 14:53:34. No Nix builders were present at the earlier healthy pre-reset inspection; subsequent captured messages were kernel device shutdown. These observations make ordinary build load less likely, but do not establish the current power state or rule out a severe stall/network failure. No live process/load reading was obtained and no power action was taken. Captures are in `load-check-20261030-1517/`.

## Owner recovery and investigation at 17:08

Owner report: **"predator was at 30w, so weird not the hanged amount. its now rebooted. check what happened."** This adds an approximately30 W observation to the startup outage, not a Q37 sleep result. Its exact time, reset method and off duration were not supplied. The healthy recovery boot is `614a8318-dc72-4308-ae89-d4680fafac4e`, btime **1793376438**, **17:07:18 CET**. It is not attributed to the earlier WoL.

Recovered old-boot evidence:

- Poweroff was requested14:53:25.149805. Nix stopped normally14:53:25.458834, with **90 ms cumulative CPU over1h15m10.151s**, **11 M memory peak**, and no pre-reset nixbld children. Heavy Nix building does not explain this recorded shutdown.
- Closing fast/tank1/tank2/tank3 encrypted mappings returned `EBUSY`; shutdown continued. The same four errors occurred during the successful13:34 shutdown. They are not discriminating evidence of this failure.
- System Power Off finished **14:53:32.800007**, then final shutdown/sync/SIGTERM; journald stopped **14:53:33.029048**. Pi netconsole continued through old-kernel device shutdown14:53:34. Software poweroff completion does not establish physical power-off.
- The persistent boot list goes directly from old boot ending14:53:33 to current boot beginning17:07:19. No intermediate boot is retained, but an unsuccessful early boot might leave no journal. Current pstore is empty; its archive service skipped. No crash record or final callback identifies a component.
- The exact cause remains **unresolved late shutdown/power transition versus early startup**. Owner30 W is distinct from prior15 W suspend hangs; neither wattage alone diagnoses a phase or driver.

At17:08:43 the recovered system was still finishing container startup with one transient healthcheck failure. By17:08:57 it had no jobs or failed units and all four Immich containers were healthy, without agent intervention. Full health at **17:09:35** passed all five authenticated boot audits and late-phase rejection, exact frozen system/kernel/modules/runtime, counters0/0, recorderN/markers0, async1/trace0/deep/pm_testnone, TPM initialized/rawECnot-run, AC1/lidclosed/fbblank4/WoLg, expected LANUUID/DNS, fresh NTP, healthy pools/five mounts/applications. No HOLD, active checked maintenance or nixbld processes. USB identifiers/serials/speeds/power states matched Q35r1 preparation byte-for-byte. Battery was full/current_now0 after recovery, not evidence of its earlier state.

The full Pi recovery snapshot is **2,294,945 bytes**, SHA256 **`c6fca9cac83fb85b66af1d3230252d79d8a2acd1f422c6277d93590d7d3d3694`**, with new-boot traffic. Earlier frozen logs remain unchanged. Its initially cached neighbor `FAILED` state coexisted with new traffic and successful authenticated SSH; a cached state alone cannot substitute for a current probe.

## Owner-boot Q37 preparation

Manifest **`65a74a97d05ba5c3eb6c28851e8835c8b499c2663a1c672b2c7ec40c7790f39a`**, saved at `preparation-ownerboot-20261030-1708/plan.conf`, pins the identities/settings above and this owner-recovery boot. No additional agent power action or baseline/runtime change was made. The existing24 tests/build remain applicable.

| Event | Epoch | October30 CET |
| --- | --- | --- |
| Boot | 1793376438 | 17:07:18 |
| First serial Q37, boot+2220 | 1793378658 | 17:44:18 |
| First wake, due+960 | 1793379618 | 18:00:18 |
| Retry | 1793379648 | 18:00:48 |
| Probe | 1793379708 | 18:01:48 |
| Frozen snapshot | 1793379798 | 18:03:18 |
| Review, rounded up from wake+600 | 1793380260 | 18:11:00 |

Manifest observation subset08:00 to20:00 is within the owner08:00 to23:00 window. This is Q/ordinal1/serial0 on owner-reset provenance, not an exact controlled-startup replication, D primer or established age bound.

The actual first Nextcloud cron started **17:12:20.158540** and completed **17:12:22.322928**, next timer17:17:20. Its five-minute interval and one-minute accuracy place the observed phase before the37-minute target. Coalescing/load can shift future firings; both maintenance preflights remain enforced, with no cron timer/service edit. Stage, harmless rehearsal and arm succeeded; arm completed17:14:47.

Independent **17:16:51** verification found the primary waiting17:44:18, service inactive, new5gqr attempt/restore paths,25-minute timeout, exact manifest hash, counters0/0, recorderN/markers0, ordinaryasync1, no requested artifact/jobs/failed units and synchronized NTP. Only this trial has future primary/Pi coverage. All four observer timers are active/waiting at the times above, nonpersistent, accuracy1s/random0; Pi receiver/NTP/waker hash are correct.

Complete baseline Pi log: **2,298,461 bytes**, SHA256 **`740c198f89d86cced600dbc579575fa104e089609211dbec7a5fcdaae4591fab`**. Setup Telegram **88804**, epoch1793376884, and owner investigation/schedule notice **88805**, epoch1793377040, were acknowledged. Dated power graph **17:43 to18:05** requested.

Exact review task: `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-serial-q37-review-20261030-1811`. Created disabled, updated with its exact self-deletion ID and full prompt after arm, then enabled with verified **`nextRunAt=2026-10-30T17:11:00.000Z`**. It was successfully deleted at review start; the date guard passed **18:11:24 CET October30**. Standing automatic continuation remains subject to health and evidence guards. Reproduce a compliant short control before age narrowing. No hung-host recovery, false age/ordinal, synthesized completion or Q-as-D-primer.

Every actual test still needs requestOk, exact boot/baseline, one ordered current-boot deep entry/resume/exit, counters1/0, markers0, current measured sleep>=900seconds, valid matching completion, fresh clock, both pools/five mounts, and its own dated low-power graph for fullPASS. The corrected verifier's raw early snapshot and final verification history must be preserved separately; no completion synthesis.

## Review at 18:11

Classification: **SUSPECTED_HANG_PHYSICAL_STATE_UNKNOWN**. The corrected controller returned partial collection, epoch1793380355, because Predator SSH exited255 with `No route to host` before executing its collection script. Observer collection and notification-status succeeded. `result-predator.txt` is empty; this is an access failure, not proof that remote artifacts are absent. No physical report or dated graph for this trial has been supplied.

The current-attempt excerpt begins at the unique CAPTURE CHECK for expected boot `614a8318-dc72-4308-ae89-d4680fafac4e`, nonce1793378658, excluding historical cycles in the full log:

| Event | Captured monotonic seconds | Pi receive time, CET |
| --- | --- | --- |
| CAPTURE CHECK, pinned system | 2219.780877 | 17:44:18 |
| First preflight, uptime2220.28, reported awake2219s, prior0/markers0 | 2220.255661 | 17:44:19 |
| Second preflight, uptime2222.67, reported awake2222s, prior0/markers0 | 2222.643302 | 17:44:21 |
| BEGIN, Q/ordinal1/pm_async0 | 2222.864509 | 17:44:21 |
| logind SuspendWithFlags request, flags1 | 2222.935366 | 17:44:21 |
| PM: suspend entry (deep) | 2223.093895 | 17:44:21 |
| Last received r8169 pci_pm_suspend call | 2223.861740 | 17:44:22 |

The immutable manifest deadline remains btime+2220; preserve the runtime's integer awake readings rather than rewriting them. The isolated excerpt has one BEGIN, one request, one deep entry, zero low-level resumes, zero exits and zero measured-duration messages. Three disk warmups and both preflight markers precede BEGIN. Unlike Q35r1's maintenance abort, this attempt reached the suspend request. Received callbacks include successful returns before the final network-driver call, but the final line is only a capture boundary. A returning Q35 also stopped netconsole at that boundary. No driver or precise failure phase is established.

Pi execution journals:

- First WoL sent **18:00:18.649018**, service finished18:00:18.664804.
- Retry sent **18:00:48.651921**, service finished18:00:48.667612.
- LAN probe started18:01:48.560479 and exited1 at **18:01:51.654964**.
- Frozen snapshot started18:03:18.568552, service finished **18:03:18.582486**.

Later inspection found some transient unit files already garbage-collected. Empty execution timestamps/default properties do not override the successful send/snapshot journals. The frozen result at `.agents-local/r-serial-q37-20261030-result.log` contains **2,331,434 bytes**, SHA256 **`41671c87b94c1f3c7ca9a4faca0aeda37496f83b961f903e5fd120779256caef`**. Its copy mtime is18:03:18.576733693. At **18:13:03**, the live log had the identical size/hash and last-write17:44:22.606292959. Pi boot2142644e-84db-40ae-b1b9-9e8c71265cf7, receiver active, NTP synchronized with packet564/offset+1.967ms, expected waker hash; only expired historical notice timers remained. Neighbor state was FAILED; the independent scheduled LAN probe and WAN SSH had also failed.

Current Predator identity, post-cycle counters/markers, request return result, raw and final verification captures, verification reader cleanup, completion/last-completed, full boot journal, NTP and storage are unavailable. No successful completion, return timestamp, measured sleep or physical low-power duration can be established. Do not infer that the verifier ran or failed, invent missing artifacts, call the scheduled960s measured sleep, or classify SHORT_SLEEP without a duration.

Telegram acknowledgements:88807start,88808wake,88809retry,88810failed-probe,88811snapshot. Review **88812**, epoch1793380421, reported suspected hang and requested current watts/LEDs/screen plus the dated **17:43 to18:05** graph. Private token/recipient values were redacted before new controller captures; the separate review acknowledgement stores only ok/message_id/date.

This short-age nonreturn does not establish a repeatable age bracket. If the owner confirms a hang at37 minutes, a long awake period was not necessary for that occurrence; it would not prove a sharp threshold, driver cause or exact comparability with Q35. Keep the unknown owner-reset provenance and changed post-return instrumentation explicit. Prior Q35's owner3 W, D's15 W hangs, Q35r1's maintenance abort and the earlier30 W startup outage remain separate records.

Automatic continuation is blocked by the unreachable host. No extra wake/reset, new manifest/suspend, runtime change or replacement polling task was created. After owner recovery, retrieve this boot's retained artifacts and journals, classify the physical evidence, then prepare one justified fixed-serial short-control replication if eligible. Age narrowing and E remain unsupported. The agent did not stop or alter the unavailable primary service or its wake coverage. Remote documentation synchronization is pending; its last known main/trial preimages are the copies in this review archive.

## Evidence and synchronization

Complete pre-reset checks, startup unit/proofs, failed transport/LAN probes, full frozen Pi capture, owner instruction/notice and preparation schedule are archived with checksums under `preparation-blocked-20261030-1451/`. The dated follow-up's health script, transport failure/status/timestamps, Pi state and complete snapshot, notification acknowledgement and pre-edit README copies are preserved under `followup-blocked-20261030-1510/`; the later requested load check is separate. New recovery evidence is in `recovered-startup-20261030-1708/`, and the actual manifest/stage/rehearsal/arm/baseline/cron/final proofs/schedule are in `preparation-ownerboot-20261030-1708/`. Review collection, errors, full Pi result, derived current-attempt excerpt, observer inspection, notices and pre-edit READMEs are in `review-20261030-1811/`. Old captures remain immutable. No checkout TMPDIR override.

Notification capture uses a scratch Rust SSH-output proxy that invokes the unchanged pinned WAN adapter with identical arguments/stdin and redacts private environment values before the controller writes setup/status output. Its source and binary hashes are retained; technical Pi captures still require exact size/SHA checks. No old evidence was redacted. Private token/recipient values are excluded from new archives.

Remote preimages were rechecked during recovery: main README `3889118e0b42971fe6fc3c4f88619c2b9a7fb405575d6c73dda421287dc741b3`; priorQ35r1 README `bc0b61b978c01ebfb42dd1ca283264b4de01920c8356a7c52be24a7b6aab7f71`. Guarded synchronization includes only those intended documentation updates and the previously absent Q37 directory, with fresh hash checks and preserved preimages. Runtime source remains `dc626ff76f258e1dd36f5f0bef4285ee0951482b135dccccf0333ca8bb779d86`; no source transfer is needed. Unrelated changes and old captures remain intact. Missing original STATE.md/PLAN.md are not reconstructed.
