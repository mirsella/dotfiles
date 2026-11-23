# Predator suspend hang

Predator (Acer Predator PH317-51, BIOS V1.21, EC 1.16) sometimes hangs when it
is asked to suspend to RAM. It stays powered at about 15 W on the wall meter,
ignores the keyboard and a short press of the power button, and comes back only
after the owner holds the power button. A good suspend draws about 3 W. Normal
running draws about 27 W.

This file is the whole study: what is known, the working theory, the method,
the test queue and the results. The first study's evidence is archived (see the
last section).

## What the first study recorded

Between October 20 and November 16 the first study made 77 real suspend
requests, almost always one per test. 63 returned, 13 hung and 1 was unclear
(an s2idle run that Wake-on-LAN never woke).

| Split | Requests | Hung |
| --- | --- | --- |
| S3, more than 8 h awake since boot or the last resume | 8 | 8 |
| S3, less than 4.2 h awake | 66 | 5 |
| S3 between 4.2 h and 8 h awake | 0 | |
| s2idle (one of them after 11 h awake) | 3 | 0 |
| Linux 6.18.58 | 54 | 11 |
| Linux 6.12.114, never tested after a long time awake | 19 | 0 |
| Ubuntu 24.04 live, Linux 7.0 | 3 | 1 |
| External hub and its disks attached | 49 | 3 |
| Hub and its disks removed | 9 | 2 |

The five hangs after a short time awake each had something unusual:
`pm_async=0`, a CPU idle-latency limit held at 2 µs, the xHCI driver unbound, a
previous suspend that returned without ever reaching low power, and the Ubuntu
live system on its second suspend. The unmodified NixOS configuration never
hung after a short time awake in about 50 requests.

Eleven dry runs (`pm_test`, where the kernel runs the suspend path and skips
the firmware step) all returned. One of them, a `core` dry run, passed after 15
hours awake. The real suspend 20 minutes later hung.

Every hang with a meter reading stayed at 15 W from the moment of the request.
The owner recorded no LED or fan observations.

## What this means

Time awake is the strongest association in the old records, not yet a proven
trigger. Every recorded S3 request after more than 8 hours awake hung. Several
suspends returned at 4 to 9 hours of uptime when each came within about 2 hours
of an earlier resume. The hub and external disks are not required for a hang.
The kernel comparison was not age-matched, so it does not establish an advantage
for either version.

The reported steady 15 W favours a failure before low-power entry, but does not
prove where execution stopped or exclude a brief sleep/resume transition below
the meter's sampling resolution. A dry run skips the final platform transition;
its return narrows the search but does not clear every kernel or driver path.
The firmware sleep handler remains a hypothesis.

## Working theory

On this machine the chipset's `SLP_SMI_EN` bit is set (`SMI_EN` reads
`0x80002033`). With that bit set, the sleep request does not put the machine to
sleep directly. It first runs a handler in the Acer firmware (system management
mode), and the firmware then performs the real sleep. A handler that never
finishes gives exactly what the owner sees: CPU powered, operating system
frozen, no reaction to keys, steady 15 W.

The theory is that this handler stalls on some piece of hardware state that
drifts over hours awake and is reset by a boot or a resume. It is unproven.
What that state is remains unknown. Candidates are the embedded controller, the
Management Engine, the discrete NVIDIA GPU (powered, with no driver, because
nouveau is blacklisted), the battery charger and disk standby.

BIOS V1.21 (November 2018) is the last release for this model, so a firmware
update is not an option.

## Method

- An aged trial is one real S3 request made at least 9 hours after the last
  boot or resume. With the production configuration it is expected to hang.
  This is the reproducer.
- Each arm changes one thing and runs as an aged trial. Two returns merit a
  reversal to the control; they do not prove a fix. Keep the system, disk layout
  and workload comparable. A changed baseline needs its own control.
- Count age from the last boot or real resume. Dry runs may also change the
  relevant state, so record them rather than assume they have no effect.
  A hang costs one power-button hold. Plan for one trial per day.
- Short-awake batches are only for checking that a change does not break
  suspend. They cannot show that a change fixes the hang.
- Read the pending trial's outcome before arming another. Reuse its matched
  baseline snapshot while the system stays unchanged, and do not repeat a
  passed short-awake safety batch unless the tested change is different.
  Recording or committing results must not reset the awake clock.
- `pm-batch snapshot` records hardware and power state. Compare a fresh
  snapshot with an aged one to find what drifts.
- Trials run from a timer on Predator, so they do not depend on main, the
  network or an agent session. `pm-batch` refuses to start unless the pools are
  healthy and `SLP_SMI_EN` is set, and it waits up to ten minutes for systemd's
  job queue to empty. Keep trials away from 23:00 to 00:30, when the backups
  run.
- Pin a timed trial to its prepared boot, system and suspend counters, and
  check its age again before it starts. Abort for blocking inhibitors, active
  storage scans or backup jobs. For new trials, sort the saved and live mount
  lists with the same locale (`LC_ALL=C`) and verify the comparison before arming.
  A cancelled timer or missing log is not a hang. Do not restart a trial
  cancelled for another task without checking that task has finished.
- Require a passing `predator-boot audit` before arming a trial. If an
  activation changed the system after boot, wait for an owner-approved reboot
  and repeat the health checks. Do not bypass the audit or reboot automatically.
- A one-time T3 scheduled task in the study's thread reads each trial
  afterwards, records it here, arms the next trial and schedules its own next
  run.
- Commit the study's text and source changes after each iteration, including
  results, cancellations, blockers and any queue changes. Review the exact
  staged diff and exclude other tasks' changes, even when already staged.
  Keep raw logs, snapshots, binaries, archives and private recovery material
  outside Git. Commit from the available study checkout even if Predator is
  hung. Do not create duplicate commits in both checkouts or push unless asked.
- Nothing recovers a hang automatically. The owner holds the power button.
  Predator has a working battery, so cutting mains power does not reset it.
- After each hang, ask the owner for three things: the meter graph around the
  trial time (did it dip toward 3 W at all), the power LED, and whether the fan
  was running.

## Test queue

Work through this in order. Record every run in the results table.

1. The November 17, 02:00 trial never ran. Another session stopped
   `pm-aged-1.timer` on November 16 at 17:40:09 during storage migration.
   No `aged-1` logs exist, and the old boot's journal has no suspend after
   November 16 at 16:20. This is a cancellation with zero requests.
2. `aged-3` fired November 18 at 06:00 and refused before any requests because
   `/run/current-system` no longer matched its prepared generation 183.
   After Predator booted generation 189, `aged-4` fired at 21:00 but failed
   before making any requests. `aged-5` reproduced the hang on generation
   189: all five dry cycles returned, then the real S3 request did not.
   This is enough to move on to a changed arm without repeating this control.
   Neither `aged-3` nor `aged-4` tested suspend.
3. The matched `fresh-4` and `aged-5-before` snapshots show changes in PCH
   Management Engine power-state readings, the ME status data and EC RAM.
   PCI/USB power-state readings, disk power modes and the captured PCH
   registers were unchanged. The snapshots are single samples at different
   loads and temperatures; the differences are leads, not causes.
   `fresh-1` and `fresh-2` are unmatched to the changed deployment.
   `fresh-3`, captured November 17 at 15:05:33, predates Ente activation;
   `aged-3-before` was never created. Do not treat them as a matched pair.
4. Firmware handler bypass, short awake. `nosmi-short-1` stopped before
   making a request. The launcher now checks the expected log files for
   each stage independently of the counters. The short safety batch did
   not pass and will not be repeated after the aged probe failed.
5. Firmware handler bypass, aged. The owner approved one probe on the
   already-aged boot. The runner read back `SLP_SMI_EN` clear before
   suspend, but the request did not return and the owner found Predator
   hung at 15 W. Clearing the bit before the request was not sufficient.
   This does not rule out firmware involvement elsewhere. Drop the
   bypass arm: no short batch, formal aged repetitions or reversal are
   needed. Pause before changing arms.
6. Discrete GPU powered off, aged. The awake check put the unbound graphics
   function `0000:01:00.0` and its upstream port `0000:00:01.0` into D3cold
   with `power/control=auto` on the GPU alone. The permanent NixOS rule
   was not active during this trial. Before an aged GPU-off trial,
   verify both devices reach D3cold under the policy being tested and verify
   the rest of the baseline. Do not count S3 with the GPU still powered as
   GPU-off. `gpu-off-1` did not return even with both devices in D3cold
   at the request boundary. The owner saw 15 W and rebooted. GPU-off
   reduced awake wall draw in the meter test but was not sufficient to
   prevent this hang. Skip the repeat and reversal and move to step 7.
7. s2idle, aged. It returned once after 11 hours awake.
   Wake-on-LAN did not wake s2idle on this machine; an RTC alarm has
   brought it back. `s2idle-aged-1` refused before any request because
   the booted and active generations differed. This did not test s2idle.
   `s2idle-aged-2` was cancelled before its trigger after another generation
   change. `s2idle-aged-3` also refused before any request: generation
   205 was active while generation 202 was booted. On the audited
   generation 205 boot, `s2idle-aged-4` returned at the expected RTC
   interval. The 25-minute repeat on the same baseline also returned
   at the expected RTC interval. The owner reported about 7 W during
   the repeat. The two-hour trial refused before any sleep request.
   `s2idle-long-boot-1` was cancelled after generation 206 was activated
   while generation 205 was still booted. `s2idle-long-boot-cold-1`
   is armed on the audited generation 206 boot, with the system's
   existing D3cold GPU policy. The seven-hour run remains unarmed
   until a verified two-hour wake on schedule and a healthy, unchanged
   system. Meter readings are not required to advance.
8. Threshold. Trials at 5, 6 and 7 hours awake. A sharp threshold points at a
   timer or counter, a gradual one at a slow drift.
9. Arms from the snapshot differences in step 3.

If s2idle proves unreliable or uses too much power, the fallback is a
clean shutdown and timed power-on. Wake-on-LAN has powered the machine
on from off; RTC wake from s2idle does not establish RTC power-on from
shutdown. Leave S3 threshold and snapshot trials paused while validating
s2idle.

## Running a trial

The runner is [pm-batch.rs](pm-batch.rs), a single Rust file with no
dependencies. Predator has no Rust toolchain, so build a static binary on main
and copy it over:

```sh
rustc -O --edition 2021 -D warnings --target x86_64-unknown-linux-musl \
  hosts/predator/pm-study/pm-batch.rs -o .scratch/pm-batch-build/pm-batch-musl
scp .scratch/pm-batch-build/pm-batch-musl \
  predator:/home/mirsella/dev/scratch/predator-suspend/pm-batch
```

Predator's login shell is nushell. Run remote commands through
`ssh predator /run/current-system/sw/bin/bash -s`. On Predator, with
`B=/home/mirsella/dev/scratch/predator-suspend/pm-batch`:

```sh
# Find when the clock was last reset.
journalctl -k -b -g 'PM: suspend exit' -o short-iso | tail -1
date --date="@$(awk '/^btime / {print $2}' /proc/stat)" --iso-8601=seconds

# A short batch now: 3 real suspends, 60 s alarm, 30 s between cycles.
sudo systemd-run --unit=pm-batch-LABEL --collect $B LABEL s3 3 60 30
```

For a guarded trial, build [pm-aged-control.rs](pm-aged-control.rs) with
the same `rustc` flags as the runner. Set the service's `ExecStart` to the
binary followed by
`run <label> <boot-id> <system> <runner-sha256> <storage-records>`. Use a
one-shot calendar timer and runtime unit files in `/run/systemd/system/`.
Do not embed a shell program in `ExecStart`; systemd expanded Bash parameters
in the `aged-4` command before Bash could read them. Use `check` instead of
`run` to verify all initial guards without running the batch. Timers are
non-persistent, with no restart or recovery action; a reboot removes them.

### `nosmi-short-1` scheduling check

`pm-nosmi-short-1.timer` fired November 19 at 01:30 CET. The T3 precheck
ran at 01:10 CET and the results check at 01:55 CET. The prepared binary,
two unit files and `SHA256SUMS` are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-nosmi-short-1-20261119/`.
`pm-aged-control` selects the short bypass for a `nosmi-short-N` label. It
requires a boot under four hours old with suspend counters at 0/0, takes a
before snapshot, repeats the preflight and then runs three S3 requests
with 60 s RTC alarms and 30 s gaps. It takes an after snapshot if the batch
returns. It does not run any dry cycles.

Verify all three prepared hashes, compare the loaded units with their saved
copies and inspect `systemctl cat pm-nosmi-short-1.service`. As root, invoke
the prepared binary with the **same arguments as `ExecStart`**, replacing
`run` with `check`. This checks the pinned baseline, age, counters, storage,
inhibitors, maintenance, boot audit and absence of trial logs or STOP. Do not
start the trial service or run a batch at precheck.

For future trials, repair only before the scheduled time, if no one
cancelled the trial, the checks pass, the service is inactive or missing,
and only scheduling is wrong. Do not rearm the concluded `nosmi-short-1`.
If the timer is missing, install its saved unit into `/run/systemd/system/`,
restoring the service from its saved copy only if it is also missing. If
only the time is wrong, keep the service unchanged and set
`/run/systemd/system/pm-nosmi-short-1.timer.d/schedule.conf` to:

```ini
[Timer]
OnCalendar=
OnCalendar=2026-11-19 01:30:00 Europe/Paris
```

Then reload systemd and start or restart only the timer. Confirm the trigger
with `systemctl show pm-nosmi-short-1.timer -p NextElapseUSecRealtime`.
If a check fails or runs late, report the blocker without changing the
baseline or replaying a trial.

### `nosmi-aged-probe-1` schedule

On November 19 the T3 precheck is at 08:30 CET, the probe is at 08:40 CET,
and the results check is at 09:00 CET. Prepared files are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-nosmi-aged-probe-1-20261119/`.
The `nosmi-aged-probe-N` mode requires at least nine hours since boot and
0/0 counters. It takes a before snapshot, checks the baseline again, and
attempts one S3 request with a 120 s RTC alarm and firmware bypass.
There are no dry cycles. Use the same checksum, loaded-unit and read-only
`check` procedure as for the short arm, with the probe's own unit names
and arguments. If the precheck fails, report without repair or replay.

Check the meter graph around 08:40 separately from the software result.
If the probe hangs, leave recovery to the owner and pause the arm. If it
returns normally, the next step is still a three-cycle short check on a
fresh boot, not a formal aged repetition.

The probe did not return, and the owner confirmed the 15 W hang and forced
a reboot. Do not replay the probe or arm the short batch or aged repetitions.
The follow-up is the awake GPU check below, not another bypass trial.

### `gpu-awake-1` schedule

The T3 check starts November 19 at 10:00 CET. It does not suspend. On the
09:17:07 generation 189 boot, the GPU was unbound with `power/control=on`
and the upstream port was already `auto`. Both were active in D0, both
allowed D3cold, suspend counters were 0/0, and the audit and pool health
checks passed.

Recheck the pinned boot, generation, counters and normal health guards
before changing anything. Timestamp the baseline and the change, enable
runtime PM on only the graphics function for roughly two minutes, and
verify the port and GPU power state. Restore `on` even if the check fails.
Do not change the audio function, bind a driver or stop services. Compare
actual wall readings for the observed interval, not the planned time.

The awake check confirmed D3cold. The owner saw a drop from about 30 W
to 17 W near the 10:03:19 to 10:05:05 CET test, but the meter reports
five-minute intervals. That is consistent with lower power draw, not a
measurement of how many watts D3cold saved. Do not switch to s2idle on
the basis of this coarse reading.

Predator's NixOS config now allows runtime power management for the
unbound NVIDIA graphics function on device add. This is the same setting
used by the awake check; it does not force the GPU off if the kernel needs
it. It has not been activated. After booting the configuration, verify
both PCI devices reach D3cold and treat the new policy as a baseline
change before arming any suspend trial.

### `gpu-meter-1` awake comparison

The longer measurement uses the same generation 189 boot and changes
only the GPU's `power/control`. The launcher accepts `gpu-meter-N` to
hold the GPU and port in D3cold for 25 minutes, sampling every 30 seconds.
It does not invoke a suspend batch. Run it only under its systemd service:
`RuntimeDirectory` holds the restore marker and `ExecStopPost` returns
the GPU to `on` and checks both devices reach D0, including after an error
or timeout. The unit does not restart. A short interval at either end
can include both D0 and D3cold, so compare complete five-minute bins.

On November 19, both devices reached D3cold at 10:29:27 CET, stayed
there through the last sample at 10:54:27, and were back in active D0
with the original policy at 10:54:29. The 11:05 read-only check
counted 50 D3cold samples over exactly 1,500 seconds, with no gap
longer than 31 seconds. The journal confirmed successful
`ExecStopPost` restoration. Both devices were still active in D0
with the GPU set to `on`. The pinned boot and generation 189
links were unchanged, and suspend counters were still 0/0.
All nine volumes passed the boot audit, the three healthy pools
and five mounts matched the baseline, and all four Ente services
were running with their original PIDs and activation times.

The owner's Tapo graph showed about 17 W from 10:30 to 10:50,
entirely inside the D3cold interval. The historical trace was around
30 to 32 W shortly before the change and climbed back to about 33 W
after D0 was restored. That is roughly 13 to 16 W lower at the wall
during D3cold in this run, not a workload-matched estimate of GPU power.
Exclude the 23 W "Now" point at 11:11: the owner has seen that endpoint
fluctuate, and it is not confirmed to represent a complete interval.
The separate 18:40 suspend timer was left unchanged.

### `gpu-off-1` aged S3

On November 19, the T3 precheck is at 18:20 CET, the host timer
fires at 18:40 CET, and the T3 results check is at 19:05 CET.
The owner moved the trial forward from 20:00. The new trigger
is 9 h 23 min after the 09:17:07 boot.
Prepared unit files, binaries and checksums are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-gpu-off-1-20261119/`.
Read-only `prepare` checked the projected age and pinned baseline
before arming; `check` and `run` require the actual minimum age.

The service takes a snapshot and runs the same five D0 core dry
cycles as `aged-5`. After the second preflight it changes only the
GPU's runtime PM policy and requires both devices to reach D3cold.
The separate `pm-batch-gpu-off` runner rechecks both states just before
logging `enter` and issues one real S3 request with a 120 s RTC alarm,
without firmware bypass. `ExecStopPost` restores GPU `on` and verifies
both devices reach D0 if the machine still responds. A hang still
requires the owner to recover it.

At precheck, compare the loaded units and installed binaries with
the prepared copies and checksums. Use the stored `ExecStart`
arguments with `check` in place of `run`. Do not start the service,
change GPU policy, repair or replay a failed or cancelled trial.
The 19:05 check was scheduled to read the actual request and meter
evidence before choosing the next arm.

The 18:20 precheck confirmed the exact 18:40 CET trigger. The trial
service was inactive and had not started. Both runtime units and
installed binaries matched the prepared copies, all four SHA-256
checks passed, and neither unit had drop-ins. There were no trial
logs or STOP file. Running only `check` with the stored arguments
passed at 32,624 seconds awake, including the boot audit for all nine
volumes. Counters stayed at 0/0, both PCI devices remained active in D0,
and all four Ente services were running. The timer and service were
left unchanged; the existing 19:05 results check was not duplicated.

At the results check, SSH via the alias and direct LAN address failed
from 19:05:35 through 19:16:12 CET. The Pi also received no ping replies,
and its netconsole log had no new data since November 17. The trial
journals and logs were inaccessible at that point. No recovery command
was sent and no next trial was armed while the result was unverified.

The owner reported a 15 W hang and rebooted Predator. A brief dip toward
3 W, LED state and fan state were not reported. SSH returned at 19:34:45
CET. The prior boot's logs show the service started at 18:40:00 and
both preflights passed. All five D0 `core` cycles returned in 7.5 to
11.8 seconds, each with a success delta of one and no failure delta.
The launcher verified D3cold at 18:42:05.497, and the runner rechecked
both devices before logging `enter` at 18:42:05.623, 33,895.82 seconds
after boot. The request used `deep`, `pm_test=none`, a 120 s RTC alarm,
and `SLP_SMI_EN` set (`smi_en_low=0x33`). Counters were 5/0 before the
request. There was no `return`, `done`, after snapshot or recorded
`ExecStopPost`. The persisted kernel journal contains only the five dry
exits. This establishes a failure to return, not whether the machine
reached physical S3 or where it stopped.

After recovery all three system links still matched generation 189,
all nine volume audits passed, and the three healthy pools and five
mounts matched the saved baseline. Counters were 0/0, `deep` was
selected, the RTC alarm was empty, and both devices were active in D0
with the original policies. Startup settled with no failed units or
queued jobs; all four Ente services were running.

### `s2idle-aged-1` with RTC wake

The November 20 host timer was set for 05:00 CET, 9 h 27 min after the
19:32:56 boot. The T3 precheck was at 04:40 and the results check at 05:25.
Prepared copies and hashes are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-aged-1-20261120/`.
Read-only `prepare` passed all guards with the projected age; `check`
and `run` enforce actual age. The sole change from the aged control is
the mode of the real request: the original runner uses `s2idle` once
with a 120 s RTC alarm, after the same five `core` dry cycles. Neither
GPU policy nor `SLP_SMI_EN` is changed. On software return the runner
restores `deep` and clears the alarm. No wake-on-LAN is planned.

The precheck compares the unit files and launcher to the saved copies,
verifies the pinned runner, then uses the exact stored `ExecStart`
arguments with `check` in place of `run`. It does not start, repair or
replay the trial. The results check records the dry cycles, real return
timing, restoration and health separately. Ask for wall draw around
the actual request; a two-minute sleep may be mixed with awake draw in
a five-minute meter bin. Do not treat a software return as a low-power
measurement.

At the November 20 04:40 precheck the timer was still waiting for
05:00 CET, and the trial service had not started. The prepared checksums
passed, both loaded units and the installed launcher matched their
prepared copies, there were no drop-ins, and the runner hash matched.
`check` refused with `system changed: /run/current-system`. The running
boot was still generation 189, but the current system and system profile
pointed to generation 190, registered at 00:18:43 CET. This check did
not reach the remaining preflight gates. Counters stayed at 0/0, there
were no trial logs or STOP file, and Ente was running and all pools
were healthy. The timer and guards were left unchanged.

The timer fired at 05:00:00 CET. The service exited with status 1
after logging `system changed: /run/current-system` at 05:00:00.610802.
It refused before taking a snapshot or running any dry or real sleep
request. At 05:25 the same boot and generation mismatch remained, with
no trial logs or kernel suspend messages, counters at 0/0, `pm_test`
set to `none`, `deep` selected and no RTC alarm. The boot audit
refused with `activated generation not booted`. All three pools were
healthy and their GUIDs and five mounts matched the baseline. No scans
or systemd jobs were active, and Ente was running. The completed study
service was the only failed unit. This was a refusal, not a hang. No
trial was replayed, and the 25-minute repeat was not armed.

At 10:25 CET the boot was still generation 189 and the active system
and profile had advanced to generation 193. The audit still refused
with `activated generation not booted`; counters remained 0/0 and no
study timer was armed. The next trial is still the short RTC check,
under a new label after an owner-approved boot and a passing audit.

### `s2idle-aged-2` on generation 193

The owner approved a reboot into the already-installed generation 193.
The reboot request was sent at 10:27:35 CET on November 20. Predator
booted at 10:29:20 with boot ID `608fda18-fca9-48ad-86db-4b93809d7275`.
All three system links matched generation 193, and the audit passed
for all nine volumes. The three pool GUIDs and five ZFS mounts matched
the pre-reboot records in
`/home/mirsella/dev/scratch/predator-suspend/preboot-20261120-generation193/`.
After startup settled, Ente was running, Nextcloud was out of
maintenance, all pools were healthy, and no systemd jobs or failed
units remained. The GPU and its port were active in D0 on their
original policies. No configuration was deployed.

The new timer is set for November 20 at 20:00 CET, about 9 h 31 min
after boot. The T3 checks are at 19:40 and 20:25. The trial uses the
same five D0 `core` dry cycles and one `s2idle` request with a 120 s
RTC alarm. It applies no GPU or firmware changes. Read-only `prepare`
passed against the new boot, original runner and saved storage
records. The verified one-shot timer is nonpersistent and the direct
service does not restart. The service had never started at arming.
Prepared unit files, launcher and checksums are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-aged-2-20261120/`.

At the November 20 19:40 CET precheck, the timer was waiting for exactly
20:00 CET and the service had never started. Both loaded units and the
launcher matched their prepared copies, the checksums and pinned runner
hash passed, and there were no drop-ins. The stored command matched the
original pins. There were no trial logs or STOP file, and the suspend
counters were still 0/0.

Predator was still on the pinned generation 193 boot, but the active
system and profile pointed to generation 202, registered at 19:34 CET.
The system pin already failed, so the launcher was not invoked, even
in `check` mode. Ente was running and all pools were healthy. The
timer and guards were left unchanged at the precheck, and no suspend
was requested.

The owner then asked to reset the test and reboot if needed. At
19:47:34 CET, the old timer was stopped before its 20:00 trigger. The
service had never started, the counters were 0/0, and no trial logs
existed. The old 20:25 results check was disabled. This was a
cancellation, not a hang.

The default boot entry was the signed generation 202 UKI with its PCR
signature companion. The pool GUIDs and five ZFS mounts matched the
previous records. Before rebooting, those identities were saved in
`/home/mirsella/dev/scratch/predator-suspend/preboot-20261120-generation202/`.
One owner-authorized reboot was requested at 19:47:34 CET. No
configuration was deployed.

The owner found Predator powered off and turned it back on. As of
20:05 CET, neither SSH path could reach it and the Pi saw no reply on
the LAN. This occurred before any sleep request. The old boot journal
records `systemctl reboot` and `Reached target System Reboot`, not a
power-off request. It ends during final shutdown before recording the
reset. There were no saved kernel crash records in pstore on the new
boot. The cause of the failed restart is unknown.

### `s2idle-aged-3` on generation 202

The new boot began November 20 at 20:04:14 CET, with boot ID
`42f5b5af-52a5-4f75-ac52-005e9775caba`. At 20:10, SSH worked and the
booted system, active system and profile all matched generation 202.
The audit passed for all nine volumes. All three pools were healthy,
and the pool GUIDs and five ZFS mounts matched the pre-reboot records.
Ente was running, there were no failed units or jobs, both GPU devices
were active in D0 with their original policies, and suspend counters
were 0/0.

The replacement one-shot was armed for November 21 at 06:00 CET
(35,746 s after boot), with checks at 05:40 and 06:25. The plan was
the same five D0 `core` dry cycles and one
`s2idle` request with a 120 s RTC alarm, without GPU or firmware
changes. Read-only `prepare` passed with the new pins and projected
age. The timer is nonpersistent, and its direct service has no restart
and had never started. Prepared units, launcher and checksums are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-aged-3-20261121/`.

At the 05:40 CET precheck, the timer was still waiting for exactly
06:00 and the service had never started. Both loaded units and the
launcher matched their prepared copies. The checksums, pinned runner
hash and stored command matched, with no drop-ins, trial artifacts or
STOP file. Suspend counters were still 0/0.

The boot ID and booted generation 202 still matched, but the active
system and profile pointed to generation 205
(`/nix/store/40ys5vy3v2rv0nzmclwhhk07bhlzzzwc-nixos-system-predator-26.11.20261023.3ec6363`).
The system pin failed, so `check` was not invoked and the remaining
launcher guards were not evaluated. Ente was running and all pools
were healthy. The timer and guards were left unchanged; no sleep was
requested at the precheck.

The timer fired at 06:00 CET. The service logged `system changed:
/run/current-system` at 06:00:00.416040 and exited with status 1.
It refused before taking a snapshot or making any dry or real sleep
request. At 06:25 the generation mismatch remained, the one-shot
timer was inactive, and the trial service was the only failed unit.
There were no trial artifacts or kernel sleep messages, counters
remained 0/0, `pm_test` was `none`, `deep` was selected, and the RTC
alarm was empty. The runner hash matched.

Ente was running. All three pools were healthy, no scans or jobs were
active, and the pool GUIDs and five ZFS mounts matched the saved
records. The only inhibitor was NetworkManager's normal delay.
The boot audit refused with `activated generation not booted`.
This was not a hang and did not test `s2idle`. Do not replay the
trial or arm the repeat on this baseline.

### `s2idle-aged-4` on generation 205

At 09:52:51 CET on November 21, an owner-approved reboot was requested.
The previous journal reached `System Reboot` and stopped during final
shutdown. Predator did not return on its own, and the owner brought it
back with Wake-on-LAN. The journal does not show why it failed to
restart. No suspend request or deployment was made.

The new boot started at 09:59:32 CET
(`efadec09-8d29-4c2f-b0e6-8fca6f55965d`). Booted system, active system
and profile all matched generation 205. All nine volumes passed the
boot audit. The three pools were healthy, their GUIDs and all five
ZFS mounts matched the records in
`/home/mirsella/dev/scratch/predator-suspend/preboot-20261121-generation205/`,
and Ente was running with no failed units or pending jobs.

The nonpersistent one-shot was armed for November 21 at 19:30 CET,
34,228 s after boot. The read-only precheck is at 19:10 and the results
check is at 19:55. The trial uses the same five D0 `core` dry cycles and
one `s2idle` request with a 120 s RTC alarm, with no GPU or firmware
changes. Read-only `prepare` passed; the service was still inactive and
had never started, and suspend counters were 0/0. Prepared units,
launcher and hashes are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-aged-4-20261121/`.

At 19:10 CET the timer was waiting for exactly 19:30 and the service
had never started. Both unit files and the launcher matched the
prepared copies, the checksums and pinned runner hash passed, and the
stored command matched. There were no drop-ins, trial logs or STOP file.
The boot and all three system links still matched generation 205,
and suspend counters were 0/0. Running only `check` with the stored
arguments passed at 33,071 s awake, including the audit of all nine
volumes. All pools were healthy and Ente was running. No unit or
setting was changed and no sleep was requested at the precheck.

The timer fired at 19:30. All five D0 `core` dry cycles returned
normally in 6.6 to 8.4 s, with one success and no failures each.
The real `s2idle` request was logged at 19:32:05.963 CET and
returned at 19:34:08.027 CET, 122.1 s later, consistent with the
120 s RTC alarm. The GPU policy stayed unchanged and there was no
firmware bypass. Cleanup and the service exit succeeded.

At 19:55, the boot, generation, runner, pool GUIDs and mounts still
matched. Counters were 6/0, `deep` was selected, `pm_test` was `none`,
and the RTC alarm was empty. Both GPU devices were active in D0.
The audit passed for all nine volumes, all pools were healthy with no
active scans, and Ente was running. There were no failed units, jobs
or conflicting study timers, and no blocking inhibitors. The short
return passed the gate for a repeat, but did not measure wall power.

### `s2idle-meter-1` on generation 205

The repeat requires five normal dry returns and one normal `s2idle`
return at the 120 s RTC interval. It also requires successful cleanup,
`deep` restored, an empty RTC alarm, a healthy
unchanged baseline and suspend counters at 6/0. A mixed or missing
meter bin from the short run is not a blocker. Stop for a refusal,
early return, hang, incomplete evidence or failed health check.

For the repeat, keep the same boot, system, runner, storage and GPU
policy. The candidate launcher is saved without a unit or timer in
`/home/mirsella/dev/scratch/predator-suspend/candidate-s2idle-meter-1/`.
Verify its SHA-256 before installing. It accepts a post-resume uptime
argument after the storage-record directory. After confirming the
return on the expected boot, read `/proc/uptime` and round its first
field up to a whole second. Use that reading as the conservative age
anchor, not boot time. `prepare` takes the planned ISO timestamp after
the anchor; `check` and `run` take just the anchor. Runtime guards pin counters at
6/0 before the dry cycles and 11/0 before the real request, and require
nine hours awake since the anchor. No stage may start at or after
22:15 CET or before 00:30 CET, so the 25-minute run has time to finish
before backups.

The post-resume observation on November 21 at 19:58:05 CET was
35,913.06 s uptime, rounded up to the anchor `35914`. Read-only
`prepare` passed for both slots, projecting more than nine hours
since the anchor. The nonpersistent one-shot was armed for November 22
at 05:30 CET, with a read-only precheck at 05:10 and results at 06:15.
The direct service had no restart. Prepared
units, launcher and hashes are under
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-meter-1-20261122/`.
The repeat uses five D0 dry cycles and one `s2idle` request with a
1,500 s RTC alarm, without GPU or firmware changes. Compare only
complete five-minute meter bins within the logged sleep interval
with adjacent complete awake bins. Do not arm another trial from
this result check.

At 05:10 CET the timer was waiting for exactly 05:30 and the service
had never started. Both runtime units and the launcher matched the
prepared copies. All checksums, the independently pinned launcher
and runner hashes, and the stored command matched. There were no
drop-ins, trial logs, STOP file, pending jobs or conflicting timers.
Generation 205, the boot ID and counters 6/0 still matched the pins.
The read-only `check` passed 33,158 s after the uptime anchor,
including the audit of all nine volumes. All pools were healthy and
Ente was running. No units or settings changed and no sleep was
requested at the precheck.

The timer fired at 05:30. All five D0 `core` dry cycles returned
normally in 6.4 to 8.5 s, each with one success and no failures.
The counters went from 6/0 to 11/0. The runner logged one `s2idle`
request at 05:31:57.611 CET, with a 1,500 s RTC alarm and no GPU or
SMI override. The kernel recorded entry at 05:31:57.614650 and exit
at 05:57:00.041572 CET. The runner recorded its return at
05:57:00.042 CET, 1,502.4 s after the request, consistent with the
alarm. The real cycle added one success and no failures. Cleanup and
service exit succeeded.

At 06:15, the counters were 12/0, `deep` was selected, `pm_test` was
`none`, and the RTC alarm was empty. Both GPU devices were active in
D0 with their original policies, and the low byte of `SMI_EN` was
still `0x33`. The boot, all generation 205 links, runner hash, pool
GUIDs and five ZFS mounts matched. All nine volumes passed the boot
audit, all pools were healthy, and Ente was running. No scan, failed
unit, job, blocking inhibitor or study timer was active.

For the meter comparison, use the four complete five-minute intervals
from 05:35 to 05:55 CET. Compare them with awake intervals such as
05:25 to 05:30 and 06:00 to 06:05. Leave out the intervals that
contain entry or return. The owner reported about 7 W during the
sleep interval but did not provide individual bins or a matched awake
reading. No further trial was armed.

### Longer s2idle trials on generation 205

`s2idle-long-1` was armed for November 22 at 15:30 CET, with a read-only
precheck at 15:10 and a results check at 17:50. It keeps the same boot,
system, storage and original GPU and firmware settings as the successful
25-minute trial. The post-resume snapshot measured 71,847.62 s uptime
just after that return, rounded up to anchor `71848`. Read-only `prepare`
passed for both slots, including the audit of all nine encrypted volumes.
The launcher checks again at runtime that at least nine hours have
elapsed since the anchor. It requires counters 12/0 initially and 17/0
after five D0 `core` dry cycles. The one real request uses a 7,200 s RTC
alarm; a normal return leaves counters 18/0. No stage may start at or
after 20:30 CET or before 00:30 CET.

Prepared units, the launcher, runner and `SHA256SUMS` are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-long-1-20261122/`.
The root-owned binaries in `/run/pm-s2idle-duration/` match those copies;
the original runner is unchanged. The one-shot is nonpersistent and the
direct service has no restart. Before sleep the extended runner reads
back the RTC alarm to verify the requested delay. The precheck can only
run `check`; it must not repair or replay the trial.

The 15:10 CET precheck was blocked: SSH timed out during the banner
exchange through the alias, directly on the LAN and via the Pi. The Pi
received ping replies from Predator. That confirmed network reachability,
not the timer, service or baseline. No launcher command ran at the
precheck and no remote changes were made.

At the results check, SSH still timed out from 17:50 through 18:00 CET
via the alias, directly on the LAN and via the Pi. Predator answered
the Pi's pings at both ends of that interval. The LAN Ente HTTPS
connection also timed out, and the Pi's netconsole log had no new
entries. The timer and service journals, sleep logs and live health
checks were inaccessible at the time, so the outcome initially remained
unverified.

After the owner's forced reboot on November 23, the previous boot's
journal showed that the timer due at 15:30 started the service at
15:36:49 CET. The launcher refused the initial preflight at 15:36:53
because `session-6909.scope` and `fwupd-refresh.service` were failed.
It exited 1. No trial artifacts or kernel suspend entries followed.
This trial made zero requests and did not test two-hour sleep.

The owner reported the fan was running at full speed before rebooting
and suspected a build. Before the trial, Hermes reported critical
memory pressure and journald flushed caches under memory pressure.
The firmware metadata refresh timed out after 16 minutes, and sshd
later logged pre-authentication timeouts and `MaxStartups` drops.
These logs support an awake resource-pressure incident but do not
identify a build or other workload as its cause.

The new boot began at 11:00:32 CET on November 23 with all system links
still on generation 205. All nine volumes passed the boot audit.
Pools were healthy, and their GUIDs and the five ZFS mounts matched
the saved baseline. By 11:04 Ente and Hermes were running, there were
no failed units or jobs, and suspend counters were 0/0 on the new boot
(`61947ed2-60a7-4db7-896e-2f03576402a4`). The old boot, counters and
age anchor cannot be reused.

The replacement, `s2idle-long-boot-1`, was armed for November 23 at
20:20 CET, with a read-only precheck at 20:05 and results at 22:40.
Read-only `prepare` passed for both slots, including the audit of all
nine volumes. It projected about 9 h 19 min of uptime at launch. This
arm requires counters 0/0 on the pinned boot before any trial activity
and at least nine hours since boot. It takes the same five D0 `core` dry
cycles, then one `s2idle` request with a 7,200 s RTC alarm and no hardware
overrides. The counters must be 5/0 before the real request and 6/0 after
a normal return. Both launch stages must start before 20:30 CET to
leave time before the 23:00 backup.

Prepared files and hashes are in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-long-boot-1-20261123/`.
The root-owned launcher and runner in `/run/pm-s2idle-duration/` matched
the saved copies at arming.

Generation 206 was activated at 12:25 CET while the pinned generation
205 boot was still running. `predator-boot audit` refused with
`activated generation not booted`. At 12:29:25, the old timer was
stopped before its trigger, with its service inactive and never started.
No trial logs existed and counters were still 0/0, so this was a
cancellation with zero requests. Both T3 checks (20:05 and 22:40) were
disabled before running. All pools were healthy and all four Ente
services were running. The default boot entry points to generation 206.

The owner approved a reboot at 12:35 CET. The previous boot reached
`System Reboot` and the journal stopped during final shutdown. Predator
did not return to the network until the owner powered it on. Generation
206 booted at 13:09:15 CET (`f9688ea4-fc8c-4b4c-a37f-523b09ab7094`).
All three system links matched, all nine encrypted volumes passed the
boot audit, and the three pool GUIDs and five ZFS mounts matched the
records in `preboot-20261123-generation206/`. Ente was running and the
suspend counters were 0/0.

On this boot the NVIDIA GPU and its upstream port were both `auto`,
runtime suspended and in D3cold under the installed power policy. The
new duration labels require that state at preflight and just before
the real request. They leave the GPU policy and the firmware handler
alone. After the dry cycles, the launcher gives the devices up to
45 seconds to settle before rechecking the whole baseline. This is
a generation 206 reliability check, not a matched D0 versus D3cold
comparison with the earlier generation 205 trials.

`s2idle-long-boot-cold-1` is armed for November 24 at 01:00 CET, with
a 00:40 read-only precheck and 03:20 results check. Nine hours after
boot falls after the evening cutoff, so the first planned slot is
after backups and midnight maintenance. Read-only `prepare` passed
for both slots, projecting about 11 h 51 min awake at the trigger.
Expect five `core` dry returns (0/0 to 5/0), then one `s2idle` request
with a 7,200 s RTC alarm (6/0 on a normal return). Both runtime units
and both binaries match the saved copies and hashes in
`/home/mirsella/dev/scratch/predator-suspend/prepared-s2idle-long-boot-cold-1-20261124/`.
The timer is nonpersistent. The service is inactive, has never
started and has no restart policy.

Use the actual entry and return timestamps for the meter comparison.
Check whether complete five-minute bins inside the sleep interval stay
near 7 W, and compare with adjacent complete awake bins. Exclude any bin
that contains entry or return. Verify RTC timing, clean restoration,
`deep` selected, no RTC alarm, unchanged storage, Ente and a passing
boot audit. A refusal, unexplained wake, hang, changed baseline or
incomplete evidence stops the queue. Do not automatically recover.

After a verified pass, automatically arm one
`s2idle-overnight-after-boot-trial-cold-1` without waiting for meter
readings. The owner reports about 15 W
during the observed hangs and about 3 W in successful S3 or 7 W in
successful s2idle. Record meter readings separately; they are not a
substitute for the wake and health checks. Target November 25 at
01:30 CET, with a 01:10 precheck and 08:50 results, only if the
actual baseline and timer inventory still allow it.
Both the precheck and trigger must be at least nine hours after a
rounded-up uptime observation following the two-hour return on the
same boot. The same launcher and runner require counters 6/0, then
11/0 after five dry cycles, and use one 25,200 s RTC alarm. A normal
return leaves 12/0. The launcher permits overnight stages only between
01:00 and 01:59 CET. Do not arm this trial ahead of the two-hour result,
change any permanent suspend policy, or schedule more arms after it.

## Trial logs

Logs are in `/home/mirsella/dev/scratch/predator-suspend/logs/`. Each batch
writes `LABEL.jsonl`, synced to disk before every request. A hang leaves an
`enter` line with no `return` line after it. Labels cannot be reused. Creating
`logs/STOP` or stopping the unit ends a batch after the current cycle. A batch
exits non-zero unless every cycle returned normally and the sleep settings were
put back.

Killing a `nosmi` batch with SIGKILL leaves `SLP_SMI_EN` clear. Later batches
refuse to start until a reboot restores it.

After a forced reboot, check the machine before the next trial:

```sh
zpool status -x; findmnt -rn -t zfs -o TARGET; systemctl --failed
sudo predator-boot audit
```

It should report healthy pools, the five ZFS mounts, no failed units and a
passing audit.

## Results

Times are CET. "Awake" is the time since the last boot or resume.

| Date | Label | Change | Awake | Result | Owner observations |
| --- | --- | --- | --- | --- | --- |
| Nov 16 15:54 | `smoke-1` | dry run ×2 | 5 min | 2 of 2 returned | |
| Nov 16 16:03 | `validate-dry-1` | dry run ×1 | 14 min | returned | |
| Nov 16 16:03 | `validate-s3-1` | none, real S3 ×2 | 14 min, 30 s | 2 of 2 returned | |
| Nov 16 16:15 | `validate-dry-2` | dry run ×2, rebuilt runner | 10 min | 2 of 2 returned | |
| Nov 17 02:00 | `aged-1-dry`, `aged-1-s3` | planned dry runs ×5, then real S3 | not applicable | cancelled before execution; zero requests | timer stopped Nov 16 17:40:09 |
| Nov 17 18:00 | `aged-2-dry`, `aged-2-s3` | planned same sequence on generation 177 and new storage layout | not applicable | timer removed by 11:48:34 reboot; zero requests | |
| Nov 18 05:40 | `aged-3` scheduling check | read-only timer and baseline checks | 14 h 38 min | timer correct; baseline blocked; zero requests | Ente left running |
| Nov 18 06:00 | `aged-3-dry`, `aged-3-s3` | planned dry runs ×5, then real S3; prepared generation 183 baseline | 14 h 57 min at timer trigger | refused before snapshots, dry runs or real S3; zero requests | moved later for Ente uploads |
| Nov 18 20:40 | `aged-4` scheduling check | read-only timer and baseline checks | 9 h 53 min | timer correct; mount guard would refuse; zero requests | |
| Nov 18 21:00 | `aged-4-dry`, `aged-4-s3` | planned dry runs ×5, then real S3 on generation 189 | 10 h 13 min | command parse error; zero requests | |
| Nov 18 21:55 | `aged-5` scheduling check | read-only timer and baseline checks | 11 h 8 min | timer correct; preflight passed; zero requests | |
| Nov 18 22:15 | `aged-5-dry` | five `core` dry cycles on generation 189 | 11 h 28 min at first request | 5 of 5 returned, no abnormal cycles | |
| Nov 18 22:17 | `aged-5-s3` | unmodified real S3, 120 s RTC alarm | 11 h 30 min | one request, no return; owner forced reboot | hung at 15 W, power LED on, fan running; any brief dip unreported |
| Nov 19 01:10 | `nosmi-short-1` scheduling check | read-only timer and baseline checks | 2 h 13 min | timer correct; preflight passed; zero requests | |
| Nov 19 01:30 | `nosmi-short-1` | planned firmware handler bypass, three S3 requests with 60 s RTC alarms | about 2 h 33 min at trigger | second preflight refused; zero requests | |
| Nov 19 08:30 | `nosmi-aged-probe-1` scheduling check | read-only timer and baseline checks | 9 h 34 min | timer correct; preflight passed; zero requests | |
| Nov 19 08:40 | `nosmi-aged-probe-1` | one S3 attempt with firmware handler bypass, 120 s RTC alarm | 9 h 43 min at request | no software return; firmware bypass did not prevent the hang | 15 W; owner forced reboot; any brief dip, LED and fan state unreported |
| Nov 19 10:03 | `gpu-awake-1` | temporary runtime PM on the unbound GPU, no suspend | about 46 min | GPU and upstream port reached D3cold; original policy and D0 restored | about 30 W to 17 W near the check; five-minute samples cannot isolate the change |
| Nov 19 10:29 | `gpu-meter-1` | 25-minute awake D3cold comparison, no suspend | about 1 h 12 min | completed and restored to D0 at 10:54:29 | about 17 W from 10:30 to 10:50 |
| Nov 19 18:20 | `gpu-off-1` scheduling check | read-only timer and baseline checks | about 9 h 4 min | timer correct; preflight passed; zero requests | |
| Nov 19 18:40 | `gpu-off-1` | five D0 core dry cycles, then one real S3 with GPU and port in D3cold and 120 s RTC alarm | 9 h 25 min at S3 request | dry 5/5; real request at 18:42:05.623 did not return | 15 W; owner rebooted |
| Nov 20 04:40 | `s2idle-aged-1` scheduling check | read-only timer and baseline checks | about 9 h 8 min | timer correct; preflight refused on generation mismatch; zero requests | |
| Nov 20 05:00 | `s2idle-aged-1` | planned five D0 core dry cycles, then one s2idle request with 120 s RTC alarm | 9 h 27 min at trigger | launcher refused on generation mismatch; zero requests | repeat not armed |
| Nov 20 19:40 | `s2idle-aged-2` scheduling check | read-only timer and baseline inspection | about 9 h 11 min | timer correct; generation mismatch; launcher not invoked | booted 193; active/profile 202 |
| Nov 20 20:00 | `s2idle-aged-2` | planned five D0 core dry cycles, then one s2idle request with 120 s RTC alarm on generation 193 | 9 h 31 min at planned trigger | cancelled at 19:47; zero requests | generation mismatch |
| Nov 21 05:40 | `s2idle-aged-3` scheduling check | read-only timer and baseline inspection | about 9 h 37 min | timer correct; generation mismatch; launcher not invoked | booted 202; active/profile 205 |
| Nov 21 06:00 | `s2idle-aged-3` | planned five D0 core dry cycles, then one s2idle request with 120 s RTC alarm on generation 202 | 9 h 55 min at trigger | launcher refused on generation mismatch; zero requests | repeat not armed |
| Nov 21 19:10 | `s2idle-aged-4` scheduling check | read-only timer and baseline checks | 9 h 11 min | timer correct; preflight passed; zero requests | generation 205 unchanged |
| Nov 21 19:30 | `s2idle-aged-4` | five D0 core dry cycles, then one s2idle request with 120 s RTC alarm on generation 205 | about 9 h 33 min at real request | dry 5/5; real return at 19:34:08 after 122.1 s; cleanup passed, counters 6/0 | no wall-power result |
| Nov 22 05:10 | `s2idle-meter-1` scheduling check | read-only timer and baseline checks | 9 h 13 min from the observation anchor | timer correct; preflight passed; zero requests | generation 205 unchanged |
| Nov 22 05:30 | `s2idle-meter-1` | five D0 core dry cycles, then one s2idle request with 1,500 s RTC alarm on generation 205 | about 9 h 34 min from the observation anchor at real request | dry 5/5; real return at 05:57:00 after 1,502.4 s; cleanup passed, counters 12/0 | owner reported about 7 W asleep |
| Nov 22 15:10 | `s2idle-long-1` precheck | read-only checks | not verified | SSH banner timed out; Pi ping succeeded; launcher not invoked | |
| Nov 22 15:30 (due) | `s2idle-long-1` | planned five D0 core dry cycles, then one s2idle request with a 7,200 s RTC alarm on generation 205 | about 9 h 40 min at refusal | launcher refused at 15:36:53 on failed units; zero requests | fan at full speed before owner reboot Nov 23; overnight trial not armed |
| Nov 23 20:20 (due) | `s2idle-long-boot-1` | planned five D0 core dry cycles, then one s2idle request with a 7,200 s RTC alarm on generation 205 | about 9 h 19 min at planned trigger | cancelled at 12:29; zero requests | generation 206 active on the generation 205 boot; both T3 checks disabled |
| Nov 24 01:00 | `s2idle-long-boot-cold-1` | five core dry cycles, then one s2idle request with a 7,200 s RTC alarm on generation 206 with the existing D3cold policy | about 11 h 51 min at planned trigger | armed; outcome pending | precheck 00:40; results 03:20; overnight unarmed |

Prepared at November 17, 09:00: no suspend in that boot; success and fail
counters both zero. Boot began around 07:33. No failed units, all three pools
healthy with no running scrub/resilver, all five expected ZFS mounts present,
Nextcloud out of maintenance, and all nine LUKS volumes passed
`predator-boot audit`. Only NetworkManager's normal delay inhibitor was present.

Rechecked November 17, 14:12: `pm-aged-2.timer` and its service are absent,
there are no aged-trial logs, and both suspend counters are zero. Pools and
mounts are healthy, no scans or backup jobs are running, Nextcloud is not in
maintenance, and no units have failed. The boot audit is blocked by an
activated but unbooted generation. No suspend trial was armed at that check.

The owner approved booting the latest installed generation and preserving the
new disk setup. Before reboot, evaluating Predator's dirty checkout produced
exactly the installed generation 183 derivation, so no rebuild, source sync,
activation or disk operation was needed. Source changes and storage identity
were saved in the private directory
`/home/mirsella/dev/scratch/predator-suspend/preboot-20261117-generation183/`.
The existing recovery bundle covers all nine LUKS volumes.

One `systemctl reboot` was issued at November 17, 14:23:19. Predator did not
return on the network. The owner reported, "it was just powered off. I have
booted it". It then booted at 15:02:38. No suspend request had been made;
this power-off is separate from the suspend hang.

At 15:05, current, booted and profile all matched generation 183. All nine
volumes passed `predator-boot audit`; all pool GUIDs, ZFS mounts and the saved
uncommitted source diff matched their pre-reboot records. All pools and Immich
containers were healthy, Nextcloud was out of maintenance, no jobs or failed
units remained, and only NetworkManager's normal delay inhibitor was present.
Suspend success/fail counters were both zero.

At November 17, 23:45, the timer was still active for 01:00 and no `aged-3`
logs existed. Suspend success/fail counters were both zero. Ente's API,
Postgres, S3 and web containers were running, but current/profile had advanced
to generation 186 while generation 183 remained booted.
`predator-boot audit` refused with "activated generation not booted".

Only the study timer was rescheduled. `pm-aged-3.timer` was confirmed active
for November 18, 06:00 CET, with 1 s accuracy, no persistence and no restart.
Its service and guards are unchanged. Ente container PIDs and activation
timestamps were unchanged across the timer update. No suspend, service
restart, deployment or reboot was performed.

T3 checks were set for November 18 at 05:40, to verify or safely repair
scheduling, and 06:30, to read results. Both have now run. The old 01:30 results
check was moved, not duplicated. No automatic recovery is configured.

At November 18, 05:40:39, `pm-aged-3.timer` was active with exactly
`2026-11-18 06:00:00 Europe/Paris` as its next trigger. The service was inactive,
its stored command matched the prepared copy, both prepared unit hashes passed,
and the runner hash was unchanged. No `aged-3` logs or STOP flag existed;
suspend success/fail counters were still zero. No repair or early dry/suspend
request was made.

The boot was unchanged, but current/profile had advanced to generation 187,
activated at 00:23:08. The audit still refused with
"activated generation not booted". All pools were healthy with no running scan;
pool GUIDs and mounts matched the saved baseline. No failed units, systemd jobs,
active checked maintenance services or blocking inhibitors were present.
Nextcloud was out of maintenance and all four Ente services were active.
The existing guarded timer was left unchanged to refuse safely. No service
restart, activation, reboot or guard bypass was performed at that check.

At November 18, 06:30:30, Predator was reachable in the same boot. The journal
records the service starting at 06:00:00 and exiting at 06:00:01 with status 1:
`Aged control refused: system changed: /run/current-system`.
This happened in the first preflight, before the runner or snapshots started.
No `aged-3` logs exist, suspend success/fail counters remain zero, `pm_test` is
none, deep is selected and the RTC alarm is empty. This is a guard refusal,
not a suspend hang or a successful control.

The one-shot timer deactivated after firing. At the check, its service was
the only failed unit. Current/profile were still generation 187, booted was
generation 183, and the audit refused with "activated generation not booted".
All three pools were healthy with no running scan; pool GUIDs and all five
mounts matched the saved baseline. No jobs, checked maintenance services or
blocking inhibitors were active. Nextcloud was out of maintenance. All four
Ente services were active with the same PIDs and activation times as at 05:40.
No services were changed, no trial was rearmed, and no further check was
scheduled then.

After the owner confirmed the upload had finished, one `systemctl reboot`
was issued November 18 at 10:41:40. Predator powered off rather than restarting.
The owner sent Wake-on-LAN, and the new boot began at 10:47:19. As on
November 17, this was before any suspend request and is not a suspend result.

By 10:49 all system links matched generation 189. The boot audit passed for
all nine volumes, and the three healthy pools and five mounts matched
`/home/mirsella/dev/scratch/predator-suspend/preboot-20261118-generation189/`.
The recovery inventory still covered all nine volumes. Ente's four services
were running, Nextcloud was out of maintenance, and no jobs, failed units,
active storage scans or blocking inhibitors remained. Suspend counters were
0/0, `pm_test` was none, deep was selected and the RTC alarm was empty.
No rebuild, activation or disk changes were needed.

Captured `fresh-4.snapshot.txt` at 10:51:16, about four minutes after boot.
Verified that `pm-aged-4.timer` was active with a 21:00 CET trigger, 1 s
accuracy and no persistence. Its service was inactive with no restart or
recovery configured. The stored guards pin the boot, generation, runner,
storage and expected counters (0/0 before dry runs, 5/0 before S3), and
include Ente's separate database backup among the maintenance checks. No
early dry or real suspend ran. Both T3 checks were moved to the new window,
not duplicated.

At 20:40:33 the timer was still active for exactly 21:00 CET. Its service was
inactive and matched the prepared copy. Both prepared unit checksums and the
runner hash matched. The boot, generation 189 links and pool GUIDs matched,
the counters were 0/0, and there were no `aged-4` logs or STOP flag. All pools
were healthy, the audit passed for all nine volumes, and no maintenance,
failed units, jobs or blocking inhibitors were present. Nextcloud was out of
maintenance and Ente was running.

The five ZFS mounts were unchanged, but the stored guard would report
`mounts changed`. The saved list puts `backups/data/recovery` before
`backups/data`, while the service sorts the live list under `LC_ALL=C`
and puts them in the opposite order. Sorting both lists under `LC_ALL=C`
confirmed that their contents match. The timer, service and prepared
baseline were left untouched. The precheck did not start a suspend.

At 21:00 `pm-aged-4.timer` fired, but systemd expanded parts of the inline
Bash program. Bash exited with a syntax error before the mount guard or
runner ran. There were no `aged-4` logs and the suspend counters remained
0/0. This was a launch failure, not a suspend hang.

The replacement starts the control binary directly and compares both mount
lists in byte order. The mount test covers reordered, missing and changed
mounts. Before arming, `systemd-analyze verify` accepted the trial units and
the binary's `check` mode passed under systemd without a dry or real request.
Only the concluded `pm-aged-4.service` failure flag was cleared. No boot,
storage, or Ente process changed, so `fresh-4` is still the matched snapshot.
`pm-aged-5.timer` was verified active for 22:15 CET with its service inactive;
the T3 checks now run at 21:55 and 22:40.

At the 21:55 check, the timer was waiting for exactly
`2026-11-18 22:15:00 Europe/Paris`. Both live units matched the prepared
copies, with no drop-ins, and all three prepared SHA-256 checks passed.
The control's read-only `check` mode passed with the trial's pinned arguments
and confirmed the boot, generation, storage and maintenance guards. The
boot audit passed for all nine volumes. There were no `aged-5` logs or STOP
flag and the suspend counters stayed 0/0. Ente was running. No repair was
needed; the timer and inactive trial service were left unchanged.

From 22:40:38 to 22:51:04, Predator was unreachable from the laptop and Pi.
The owner reported a hang at 15 W with the power LED on and the fan running,
then forced a reboot. The new boot began around 22:57. It is not yet known
whether the meter dipped before settling at 15 W.

The recovered journal shows that the timer fired at 22:15:00 and both
preflights passed. All five `core` dry cycles returned in 6.8 to 8.5 s each,
each adding one success and no failures. At 22:17:07 `aged-5-s3.jsonl`
recorded a real request with `pm_test=none`, `deep` selected and a 120 s RTC
alarm. Its last event is `enter`; there is no `return`, `done` or after
snapshot. The kernel journal has five dry-run exits and no exit for the real
request. That places the hang in the real suspend attempt, but does not
establish how far the machine got into sleep or resume.

Both matched snapshots have boot `3d404152-4566-418d-ab75-2ecec49b3b8f`.
`fresh-4` was taken about four minutes into boot and `aged-5-before` after
11 h 28 min. Several CSME blocks in the PMC readings changed from Off to On;
the second ME status word changed from `03F10506` to `83210506`, and several
EC RAM bytes differed. The captured `SMI_EN` stayed `0x80002033`, and the
other captured PCH registers were unchanged. The fresh sample was at 89 °C
CPU package temperature against 57 °C in the aged sample. This pair cannot
separate an effect of awake time from workload or temperature.

After recovery, the three generation 189 system links still matched, all
nine volumes passed the boot audit, and all three pools and five mounts
matched the baseline. By 23:00 all four Ente services were running and
no jobs or failed units remained.

At scheduling, the recovery boot and generation 189 links still matched,
the audit passed and storage matched the saved baseline. Suspend counters
were 0/0, with no trial logs or STOP. The prepared units passed systemd
verification, and the timer read back as active for 01:30 CET with the
service inactive. Runtime preflight will recheck maintenance after the
backup window. No early suspend or automatic recovery is configured.

At 01:10 the prepared checksums passed and the loaded units matched without
drop-ins. The timer was waiting for exactly 01:30 CET; the service had not
started. No trial logs or STOP existed, and counters were 0/0. The read-only
preflight passed on the pinned generation 189 boot, including the audit for
all nine volumes. Ente remained running. No repair or batch was needed,
and the 01:55 results check was left unchanged.

The timer fired at 01:30:00. The initial preflight passed, then the second
preflight refused at 01:30:16 with `trial labels already used`. Both
preflights expect 0/0 counters for this arm. The code uses zero successes
to decide when to check for any existing trial files, so the second check
rejected the before snapshot created by the same invocation. Only the
before and after snapshots exist. There is no S3 log or kernel suspend
entry, and the counters are still 0/0. No bypass was attempted.

At 01:55 Predator was reachable on the same generation 189 boot. `pm_test`
was none, deep was selected, the RTC alarm was empty, and `SMI_EN` was
`0x80002033` with `SLP_SMI_EN` still set. The audit passed for all nine
volumes. All pools were healthy with no active scan, pool GUIDs and mounts
matched, Nextcloud was out of maintenance, and all four Ente PIDs matched
the precheck. The refused trial was the only failed unit; its timer had
deactivated. No services were touched and no trial was scheduled. This run
does not call for a meter reading, recovery or an aged bypass.

At 08:21, after the owner approved the one-off probe, the new launcher's
read-only preflight passed under systemd on the same generation 189 boot,
including the nine-volume audit. Unit tests covered the initial log check
and the second-stage checks for the short batch, aged control and probe,
including rejection of extra logs and STOP. Only the concluded
`pm-nosmi-short-1.service` failure flag was cleared. The timer read back
as active for exactly 08:40 CET with the service inactive. Counters stayed
0/0 and Ente PIDs were unchanged.

At 08:30 the timer was still waiting for exactly
`2026-11-19 08:40:00 Europe/Paris`. Both live units matched the prepared
copies without drop-ins, the checksums passed and `ExecStart` was unchanged.
The trial service had not started, no trial logs or STOP existed and counters
were 0/0. Running only `check` with the stored trial arguments passed on the
pinned boot, including the audit for all nine volumes. The timer and service
were left unchanged, Ente remained running and the 09:00 results check was
left in place.

At the 09:00 results check, both SSH paths failed and the Pi saw no reply
on the LAN. Direct LAN SSH still failed at 09:04:53, 09:07:53 and 09:10:53;
the Pi still saw no reply at 09:10:57. The SSH alias failed again at
09:11:13. The Pi's netconsole log had no new data since November 17.
The passed precheck and sustained loss of reachability after the planned
trial make a hang plausible, but do not confirm that the timer fired or
that the request reached the kernel. The journal, probe logs, suspend
counters and `SLP_SMI_EN` state could not be read.

No automated recovery was sent. The owner reported a 15 W hang and forced
a reboot. A brief dip toward 3 W, the power LED and the fan state were not
reported.

After recovery, the previous boot's journal confirmed the timer fired at
08:40:00 and both preflights passed. The S3 log records the 120 s alarm
configuration, deep selected, `pm_test` none, and an `enter` at 08:40:28.452
after 35,009 s awake. The runner read back the low `SMI_EN` byte as `0x23`,
down from `0x33`, before making the suspend request. There is no `return`,
`done` or after snapshot. The persisted kernel journal has no suspend
entry or exit from this attempt, so the logs do not locate the hang in
the entry or resume path or establish that the machine reached physical S3.

The new boot began at 09:17:07. All three system links still matched
generation 189. The reboot reset the counters to 0/0; `pm_test` was none,
deep was selected, the RTC alarm was empty, and `SLP_SMI_EN` was set
(`SMI_EN=0x80002033`). All nine volumes passed the boot audit. All three pools
were healthy with no active scans, and their GUIDs and all five mounts
matched the baseline. By the final check, Ente was running and systemd
reported no failed units or queued jobs. No further trial was armed.

On November 19 the awake GPU check started from the same generation 189
boot and 0/0 suspend counters. An initial read-only attempt stopped on
journalctl's `-- No entries --` banner; it made no change. With that
banner suppressed, the preflight passed. Both the graphics function and
its upstream port were active in D0 at 10:02:49 and 10:03:04. Setting
only the graphics function to `auto` at 10:03:04 put both devices in
D3cold by 10:03:19. Both remained runtime suspended in D3cold on all eight
samples through 10:05:05. Restored `on` at 10:05:05; both devices were
active in D0 by 10:05:11.

The post-check audit and storage checks passed. Boot, generation, counters,
Ente PIDs and Ente activation times were unchanged. No suspend ran or was
scheduled. The meter history was not available during the check; the owner
later reported the nearby drop in the results table.

## Machine facts

- After the owner's recovery, the previous boot began November 19 at
  19:32:56 (boot `805078f4-eda4-41dd-858b-99b2121a61e1`). All system links
  initially matched generation 189 and the boot audit passed:
  `/nix/store/cg2x27vhsgzwb1s7djpswfsrs3qlamgz-nixos-system-predator-26.11.20261023.3ec6363`.
  At the November 20 05:25 check generation 189 was still booted, but
  current and profile pointed to generation 190:
  `/nix/store/jjhjpl0ahzarhr8rjzqzfpc08225vlds-nixos-system-predator-26.11.20261023.3ec6363`.
  By 10:25 current and profile pointed to generation 193:
  `/nix/store/fszmdsgrr83igx391w6lcgbdwbqzm3ji-nixos-system-predator-26.11.20261023.3ec6363`.
  The audit refused both times because the active generation was not booted.
- The previous boot began November 20 at 10:29:20 CET
  (`608fda18-fca9-48ad-86db-4b93809d7275`). All system links initially
  matched generation 193 and the audit passed all nine volumes. At the
  19:40 check, generation 193 was still booted, but current and profile
  pointed to generation 202. The next boot began at 20:04:14 CET
  (`42f5b5af-52a5-4f75-ac52-005e9775caba`). All three links initially
  matched generation 202 and all nine volumes passed the audit:
  `/nix/store/z0aslb72ck4b788bpg0gm13zic346c0b-nixos-system-predator-26.11.20261023.3ec6363`.
- The November 21 09:59:32 CET boot
  (`efadec09-8d29-4c2f-b0e6-8fca6f55965d`) matched generation 205 on
  all three system links and passed the nine-volume audit:
  `/nix/store/40ys5vy3v2rv0nzmclwhhk07bhlzzzwc-nixos-system-predator-26.11.20261023.3ec6363`.
- Lid closed. The owner remembers hangs with the lid open as well.
- Battery present and full. AC connected.
- Discrete GPU `0000:01:00.0`: no driver, `power/control=on`, D0.
- Six physical disks: two internal SATA SSDs and four USB HDDs. Storage work
  since November 16 changed the layout: `tank` now uses four roughly 750 GB
  encrypted HDD sections in RAIDZ1; `backups` uses the roughly 250 GB tails of
  three HDDs in RAIDZ1. `WD-WXK1A67NTCDX` is now part of these pools, not an
  unmounted NTFS disk. There are nine LUKS volumes.
- Expected mounts: `/srv/data/fast`, `/var/lib/nextcloud/data`,
  `/srv/data/archive`, `/srv/backup`, `/srv/backup/recovery`. The last two now
  come from `backups/data` and `backups/data/recovery`.
- ACPI wake sources enabled: XHC, RP03, RP04, PWRB.
- The owner's wall meter records a history graph.

## Leftovers from the first study

- The repository no longer has the first study's harness package, kernel
  modules, boot services or specialisations. [pm-study.nix](../pm-study.nix)
  now only holds the baseline every trial shares: the Acer WMI driver and the
  boot-time EC write stay off, the nightly suspend timer stays disabled, and
  the LAN profile keeps its fixed address.
- The deployed configuration no longer loads `pm_stage` or `netconsole`; the old capture,
  clock-handoff and preserve-RTC services are absent. Historical state in
  `/var/lib/predator-pm-study` is not used by the runner.
- Predator's checkout contains uncommitted storage and MCP work from other
  tasks. Leave it intact. Study commits do not authorize deploying, changing
  disks or applying chezmoi. The November 16 pre-sync state remains in
  `refs/wip/predator-20261116`.
- The first study's evidence tree and scratch receipts are archived on main in
  `~/dev/scratch/dotfiles-predator-pm-study-archive-20261116/` with a
  `SHA256SUMS` file. Predator's old receipts are in
  `/home/mirsella/dev/scratch/pm-study-receipts-20261116.tar.zst`. The harness
  code and the tracked part of the tree are in Git history up to commit
  `2446e3cc`.
