# Owner recovery and retained RTC, October 31 at 11:09 CET

**Owner-confirmed 15 W hang, followed by forced reboot.** The owner said:
"force rebooted it, it was at 15w. schedule next tests". The exact reset method,
off duration and dated power graph were not supplied. This adds physical evidence
to the immutable 10:21 review; it does not turn the failed trial into a return.

## Recovered machine and originals

At11:09:56, Predator was reachable on new boot
`ae29f675-9167-4975-a884-01885468bc38`, observed btime1793440484,
**10:54:44 CET**. This is clock-derived boot time, not an independently observed
power-button timestamp. Old boot`f8eb6e44-85ae-4e56-bc00-587876699945` retains
ten diagnostic cycles followed by one real-S3 attempt. New-boot counters0/0 and
markers0 are genuine reset state, not relabeling the old history.

The existing preserve-RTC service retained root0600 original:

`/var/lib/predator-pm-study/ae29f675-9167-4975-a884-01885468bc38-boot-rtc.txt`

Size101314bytes, SHA256
`b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55`.
It completed at boot monotonic20.734968, before timesyncd started20.743206.
Clock handoff obtained fresh synchronization at10:55:12.613483, monotonic28.155854,
and completed29.044695. The future2099 artifact mtime comes from the marker,
not a future trial date. The original bytes were retrieved and hash-checked.

The recovered trial directory contains nine original root0600 files: plan,
requested, before, before-kernel, cursor, mutating, clock-quiescence, request-call
and request-ok. `request-ok=true`; clock status changed8193 to8257 with
zero-offset timer cancellation. The plan still hashes to
`aa5be693e84beca952edddf18a7594169c3b08e3885bf61caaf4ca68d1ba5fa9`.
There is no retained raw-return, return, outcome, restored or completed file.
This absence is now established by authenticated inventory, unlike the earlier
failed SSH retrieval. It does not prove every later CPU instruction was absent.

Persistent old-boot journals retain the request and deep entry, with no return.
Their short-monotonic timestamps differ from raw kernel timestamps received by
the Pi after earlier clock accounting changes. Both sources remain intact; do
not subtract timestamps across those domains to invent a duration.

## Conditional phase decode

Early kernel RTC at boot monotonic1.382893 was **2099-07-02 00:59:40 UTC**.
The unchanged tested calendar model finds one candidate within both two hours
and24hours:

- action10, `acpi_suspend`, start=true, value3, user11, file hash118;
- encoded clock2099-07-02 00:00:00UTC plus **3580 seconds of RTC progression**;
- no retained `acpi_suspend` end marker.

Interpretation assumes device hash0, continuous RTC ticking and no intervening
firmware or other clock rewrite. Prior trace0 history, recorded quiescence,
the freezer barrier and approximately one-hour real recovery interval support
this interpretation. They do not turn these assumptions into unconditional
hardware guarantees. The new read-only decoder asserts the original raw line,
unique candidate and naive Magic value using the unchanged five-test model.

Linux's naive `Magic number: 11:598:960` and `memory11: hash matches` result from
decoding the elapsed clock without correcting it. They are not evidence of a
memory-device failure. Likewise, the final NIC netconsole callback is not the
identified culprit.

Matching frozen-kernel source places this marker immediately before the ACPI
S3 low-level call. The end marker follows successful low-level return. The
retained start narrows the boundary beyond the network capture, but cannot
distinguish entry failure, power transition, time in S3 or failed low-level
return. The3580seconds are RTC progression until reboot, **not physical sleep**.
Full PASS remains false and the dated graph remains missing.

## Recovered health

Full11:11 inspection found unchanged current/booted system, kernel and modules;
ordinary none/async1/trace0/deep, recorderN, markers0, empty RTC wakealarm;
all five authenticated boot and late-phase rejection audits passed. AC online,
lid closed, framebuffer blank4, WoLg, declared LAN/DNS, synchronized NTP/current
RTC, both healthy pools, five mounts, required applications and four healthy
Immich containers were confirmed. No jobs, failed units, builders or active
maintenance were found. No future study/nightly timer is armed. The new cron
phase was next11:14:48, not the previous boot's phase.

The kernel advertises `s2idle [deep]`; the NIC's PCI wakeup setting is enabled.
This establishes availability, not a successful s2idle wake test.

## Next scheduled experiment

An enabled current-thread task is verified for **October31 11:45 CET**,
`nextRunAt=2026-10-31T10:45:00.000Z`:

`scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-s2idle-control-20261031-1145`

It self-deletes its exact ID and prepares, validates and executes **one eligible
RTC-instrumented s2idle control**, followed by a dated review. This compares
working suspend/resume without firmware S3 against the observed S3 hang.
Serial execution and the validated RTC protocol remain; the sleep mode changes.
It is not an S3 PASS, an ordinary primer or a claimed one-component isolation
of every platform path. S2idle may draw more power than S3;15W alone must not be
called a hang if the control wakes and restores normally.

The task requires a distinct tested Rust protocol with new schema/state/runtime
pins and truthful fresh-boot history, preserving R3 and diagnostic sources.
It must verify systemd actually selects s2idle, restore deep safely, retain raw
RTC before clock writers and prove full health and independent Pi capture/wake
coverage. It must notify exact entry/wake/review times before arming with more
than300seconds lead. Pi wake is due+960, retry+30, probe+90, snapshot+180 and
self-deleting review+600 rounded up. Early wake is inconclusive for the planned
interval; no duration is inferred from a scheduled delay or NTP correction.

11:45 is the execution workflow start, not an armed suspend deadline. This turn
only retrieved recovery evidence, scheduled the workflow and synchronized
documentation. No PM/RTC setting, runtime, machine timer, wake or reset was
applied. No automatic deep retry or second power transition is authorized by
this workflow. A substantive blocker leaves the host awake and is reported.

`evidence/` preserves original RTC/trial bytes, authenticated metadata/hashes,
health, old-boot journals, tested decoder source/output, schedule and owner
notification. The deferred10:21 archive is synchronized alongside this recovery
update after remote status, flake.lock and all preimages are checked.
