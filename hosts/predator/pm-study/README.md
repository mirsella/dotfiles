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

Time awake is the trigger. A real S3 request after more than 8 hours awake has
hung every time, and a resume resets the clock: several suspends returned at
4 to 9 hours of uptime because each came within about 2 hours of an earlier
resume. Uptime, the hub, the external disks, USB activity and the kernel
version do not explain the hangs. The apparent advantage of Linux 6.12 came
from testing it only after a short time awake.

The machine never powers down in a hang, so wake-up, the RTC alarm and resume
are not involved. The kernel's part works even when aged (the 15 hour dry run).
What is left is the step after the kernel writes the sleep request.

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
- Each arm changes one thing and runs as an aged trial. One return is a hint.
  Two returns in a row are strong evidence, because a control that still hung
  70% of the time would return twice in fewer than one case in ten. After two
  returns, run one control. It should hang again.
- Every return resets the clock, so an aged trial costs at least 9 hours of
  awake time. A hang costs one power-button hold. Plan for one trial per day.
- Short-awake batches are only for checking that a change does not break
  suspend. They cannot show that a change fixes the hang.
- `pm-batch snapshot` records hardware and power state. Compare a fresh
  snapshot with an aged one to find what drifts.
- Trials run from a timer on Predator, so they do not depend on main, the
  network or an agent session. `pm-batch` refuses to start unless the pools are
  healthy and `SLP_SMI_EN` is set, and it waits up to ten minutes for systemd's
  job queue to empty. Keep trials away from 23:00 to 00:30, when the backups
  run.
- A one-time T3 scheduled task in the study's thread reads each trial
  afterwards, records it here, arms the next trial and schedules its own next
  run.
- Nothing recovers a hang automatically. The owner holds the power button.
  Predator has a working battery, so cutting mains power does not reset it.
- After each hang, ask the owner for three things: the meter graph around the
  trial time (did it dip toward 3 W at all), the power LED, and whether the fan
  was running.

## Test queue

Work through this in order. Record every run in the results table.

1. Aged control with dry runs first (armed for November 17, 02:00). Snapshot,
   five `core` dry runs, one real S3 with a 120 s alarm, snapshot. Expected:
   the dry runs return and the real request hangs. That confirms the reproducer
   on the new runner and repeats the dry-run result at the exact aged state.
   If the real request returns instead, run a plain aged control next (no dry
   runs), because the dry runs may have reset whatever drifts.
2. Compare `fresh-1.snapshot.txt` with `aged-1-before.snapshot.txt`. List every
   difference in device power states, EC memory, Management Engine status, PCH
   registers and disk power mode. Each real difference is a candidate arm.
3. Deploy the trimmed configuration, with the owner's go-ahead, then repeat
   the aged control once. Predator still runs the first study's system (see
   "Leftovers"). The deploy removes that instrumentation and brings 181 commits
   of `main`, including a newer nixpkgs, so it is a new baseline and needs its
   own control before any arm. Do it while the machine is freshly rebooted
   after a hang. Follow the Predator deploy steps in the repository's
   `AGENTS.md`, then run `chezmoi apply` there.
4. Firmware handler bypass, short awake. `pm-batch <label> s3 3 60 30 nosmi`
   clears `SLP_SMI_EN` before each request so the chipset sleeps without the
   firmware handler. Check that all three return, that the meter shows about
   3 W, and that the machine is healthy afterwards. A failed resume here needs
   a power-button hold. If this does not work, drop the arm.
5. Firmware handler bypass, aged. Two aged trials with `nosmi`. Two returns
   mean the firmware handler is the fault, and clearing the bit before every
   sleep becomes the fix.
6. Discrete GPU powered off, aged. First check awake that
   `echo auto > /sys/bus/pci/devices/0000:01:00.0/power/control` puts the port
   `0000:00:01.0` into D3cold and lowers the wall reading. Ubuntu hung with
   nouveau loaded, which counts against this arm, so it comes after the bypass.
7. s2idle, aged. It returned once after 11 hours awake. Two more aged returns
   and a wall reading near 3 W would make it a usable replacement for S3.
   Wake-on-LAN does not wake s2idle on this machine. The RTC alarm does.
8. Threshold. Trials at 5, 6 and 7 hours awake. A sharp threshold points at a
   timer or counter, a gradual one at a slow drift.
9. Arms from the snapshot differences in step 2.

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
journalctl -k -b -g 'PM: suspend exit' -o short-iso | tail -1; uptime -s

# A short batch now: 3 real suspends, 60 s alarm, 30 s between cycles.
sudo systemd-run --unit=pm-batch-LABEL --collect $B LABEL s3 3 60 30

# An aged trial from a timer.
sudo systemd-run --unit=pm-aged-N --on-calendar='YYYY-MM-DD HH:MM:SS' \
  --timer-property=AccuracySec=1s /run/current-system/sw/bin/bash -c \
  "$B aged-N-before snapshot && $B aged-N-s3 s3 1 120 30; rc=\$?; $B aged-N-after snapshot || rc=\$?; exit \$rc"
```

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
| Nov 17 02:00 | `aged-1-dry`, `aged-1-s3` | dry runs ×5, then real S3 | about 10 h | pending | |

Last resume before the aged trial: November 16, 16:05:19.

## Machine facts

- Production system: NixOS generation 175, Linux 6.18.58, booted November 16
  at 15:49 (boot `c8901e62-f8b4-4dd0-904a-c94c5ea9e111`).
- Lid closed. The owner remembers hangs with the lid open as well.
- Battery present and full. AC connected.
- Discrete GPU `0000:01:00.0`: no driver, `power/control=on`, D0.
- Six disks: two internal SATA SSDs and four USB disks on the hub. Three USB
  disks form the `tank` pool. The fourth, `WD-WXK1A67NTCDX` (NTFS), stays
  attached and unmounted at the owner's request.
- Two of the USB disks were already in standby 15 minutes after boot.
- ACPI wake sources enabled: XHC, RP03, RP04, PWRB.
- The owner's wall meter records a history graph.

## Leftovers from the first study

- The repository no longer has the first study's harness package, kernel
  modules, boot services or specialisations. [pm-study.nix](../pm-study.nix)
  now only holds the baseline every trial shares: the Acer WMI driver and the
  boot-time EC write stay off, the nightly suspend timer stays disabled, and
  the LAN profile keeps its fixed address.
- Predator has not been redeployed. Generation 175 still loads the `pm_stage`
  module, runs the old capture and clock services, and keeps their state in
  `/var/lib/predator-pm-study`. It was built from an uncommitted checkout whose
  `hosts/predator/default.nix` still trusts the retired `lantian` cache key,
  so do not build on Predator before the deploy in step 3.
- Predator's checkout now follows `main`. Its previous uncommitted state, which
  held nothing that `main` lacks, is kept there as the Git ref
  `refs/wip/predator-20261116`.
- The first study's evidence tree and scratch receipts are archived on main in
  `~/dev/scratch/dotfiles-predator-pm-study-archive-20261116/` with a
  `SHA256SUMS` file. Predator's old receipts are in
  `/home/mirsella/dev/scratch/pm-study-receipts-20261116.tar.zst`. The harness
  code and the tracked part of the tree are in Git history up to commit
  `2446e3cc`.
