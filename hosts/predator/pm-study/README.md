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
  storage scans or backup jobs. A cancelled timer or missing log is not a hang.
  Do not restart a trial cancelled for another task without checking that task
  has finished.
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
2. `aged-3` is armed for November 18, 01:00, about 9 h 57 min after the
   November 17, 15:02:38 boot. Generation 183 is now booted and passes the
   audit. The owner approved the reboot; no rebuild or disk reconfiguration
   was needed. The earlier 11:48:34 reboot removed `pm-aged-2` before it ran.
   Sequence: snapshot, five `core` dry runs, one real S3 with a 120 s alarm,
   then snapshot. Before both the dry runs and real request, guards check
   boot/system identity, expected suspend counters, age, inhibitors,
   maintenance, failed units, pool identities, mounts and the boot audit.
   If the real request returns, run a plain aged control next without dry
   runs, because they may have changed the relevant state.
3. Compare `fresh-3.snapshot.txt` with `aged-3-before.snapshot.txt` after that
   trial. `fresh-3` was captured November 17 at 15:05:33, about three minutes
   after boot. Check device power states, EC memory, Management Engine status,
   PCH registers and disk power mode. Differences are leads, not causes.
   Neither `fresh-1` nor `fresh-2` is a matched control for this deployment.
4. Firmware handler bypass, short awake. `pm-batch <label> s3 3 60 30 nosmi`
   clears `SLP_SMI_EN` before each request so the chipset sleeps without the
   firmware handler. Check that all three return, that the meter shows about
   3 W, and that the machine is healthy afterwards. A failed resume here needs
   a power-button hold. If this does not work, drop the arm.
5. Firmware handler bypass, aged. Two aged trials with `nosmi`, then reverse
   to a matched control. Returns would support involvement of the handler;
   they would not prove a safe permanent workaround without further testing.
6. Discrete GPU powered off, aged. First check awake that
   `echo auto > /sys/bus/pci/devices/0000:01:00.0/power/control` puts the port
   `0000:00:01.0` into D3cold and lowers the wall reading. Ubuntu hung with
   nouveau loaded, which counts against this arm, so it comes after the bypass.
7. s2idle, aged. It returned once after 11 hours awake. Two more aged returns
   and a wall reading near 3 W would make it a usable replacement for S3.
   Wake-on-LAN does not wake s2idle on this machine. The RTC alarm does.
8. Threshold. Trials at 5, 6 and 7 hours awake. A sharp threshold points at a
   timer or counter, a gradual one at a slow drift.
9. Arms from the snapshot differences in step 3.

If nothing isolates the cause, the fallbacks are s2idle (step 7) or a nightly
power-off with an RTC or Wake-on-LAN power-on, which already works here.

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

# An aged trial from a timer.
sudo systemd-run --unit=pm-aged-N --on-calendar='YYYY-MM-DD HH:MM:SS' \
  --timer-property=AccuracySec=1s /run/current-system/sw/bin/bash -c \
  "$B aged-N-before snapshot && $B aged-N-s3 s3 1 120 30; rc=\$?; $B aged-N-after snapshot || rc=\$?; exit \$rc"
```

The calendar example only shows the timer wiring. Add the runtime guards from
the method before arming an aged trial. Inspect the stored command with
`systemctl cat pm-aged-N.service`. Timers are transient and non-persistent,
with no restart or recovery action; a reboot removes them.

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
| Nov 18 01:00 | `aged-3-dry`, `aged-3-s3` | dry runs ×5, then real S3 on booted generation 183 and new storage layout | expected 9 h 57 min before dry runs | pending | |

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

`pm-aged-3.timer` is confirmed active for November 18, 01:00 CET, with 1 s
accuracy, no persistence and no restart. The T3 check was moved from November
17, 18:30 to November 18, 01:30 CET. No automatic recovery is configured.

## Machine facts

- Current boot began November 17 at 15:02:38
  (boot `72a5ba34-16e5-4d6f-8566-a4b6cf69e538`).
  Current system, booted system and profile are generation 183:
  `/nix/store/mgm9m1nj9rlicz4xdhxw5v177frd9g8b-nixos-system-predator-26.11.20261023.3ec6363`.
  Linux remains 6.18.58. The boot audit passes.
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
