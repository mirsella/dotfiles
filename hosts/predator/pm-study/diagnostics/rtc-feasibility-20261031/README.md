# RTC instrumentation assessment, October 31

Status: **BLOCKED_BEFORE_ARMING**. The existing RTC phase recorder can retain a
conditional last-phase clue, but the frozen host has no validated way to measure
physical sleep duration while that recorder is armed. Its RTC writeback and
reset-retention assumptions also need an explicit protocol. Preparation therefore
left the healthy host awake. It did not request suspend or change PM/RTC settings.

The October 31 task permitted one instrumented real-S3 attempt only after these
requirements passed. Its date guard passed at 08:30:22 CET and it deleted the exact
task below. The scheduler check at 08:45:49 CET found it absent and no remaining
Predator task. No real-S3 manifest, runtime, primary timer, wake timer or review
was created. There is no automatic repeat while this blocker remains unresolved.

`scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-s3-logging-preparation-20261031-0830`

## Host and evidence provenance

Read-only health captures at 08:34 and 08:45, followed by explicit guards at
08:46:00, found boot `f8eb6e44-85ae-4e56-bc00-587876699945`, nine diagnostic cycles,
zero failures and zero actual S3 attempts on this boot. The final uptime was
51567.28 seconds. `/proc/stat` now reports btime **1793381193**; October 30's
retained records report **1793381192**. Preserve this one-second clock-derived
drift and the original records. It is not evidence of another boot. The owner's
exact recovery mechanism and off duration remain unspecified.

The first 08:31 inspection stopped at a nonexistent `AC0/online` path. Both that
script and its failure are retained. The corrected script reads `ACAD/online`.
Later full captures succeeded; the failed initial capture is not a passing check.

The final captures show ordinary modes `pm_test=none`, async1, trace0, recorderN,
markers0, deep selected, empty RTC wakealarm, CPUs0..7 online, AC/lid/screen/WoL
state as expected, declared LAN/DNS and synchronized NTP, healthy pools/five
mounts and application containers. All five authenticated disk boot audits and
late-phase rejection checks passed. No HOLD, jobs, failed units, builders or
active listed maintenance were found. This is a current-health observation, not
an armed-trial preflight or a future cron guarantee.

The Pi at 08:45:50 had its receiver active, NTP synchronized, no jobs and no
future study timer. Its spent historical notification timers remain untouched.
No new capture nonce, wake coverage or trial was armed. No recovery WoL,
shutdown, reset, firmware/security change, system activation or runtime import
occurred. Frozen identities remain:

- System: `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`
- Kernel: `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`
- Modules: `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`
- Recorder: `/nix/store/3jcksrsfmyw8vk1cwp0x5ynb3h1q1pdp-predator-pm-stage-20261009/lib/modules/6.18.58/extra/pm_stage.ko.xz`, SHA256 `8fd79959fa979cabf2dfac97c4a60dbdab505cca5cf3e3902772faebd3071d9e`
- Recorder source `pm_stage.c`: SHA256 `69bdd984ae587f0d152baae8877315e4acbe705c87b5122ec7626ba71868d75d`
- Latest diagnostic binary: SHA256 `25d8e1e0530e68c2ca20b35b7c2181a6518b3775c5f5ffe703b2e3871256f750`
- Last core completion: SHA256 `7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc`
- Unrelated October 18 ordinary last-completed: SHA256 `280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08`

## What the recorder actually stores

`pm_stage.c` calls Linux `generate_pm_trace()` for selected suspend tracepoints.
It encodes an action/file hash, the low three value bits and the start/end bit
into the RTC's date and time, then writes century20 separately. It does not
retain a continuous log or an independent timestamp. A subsequent marker
replaces the previous marker. Its volatile counters do not prove the hardware
write succeeded: Linux ignores `mc146818_set_time()`'s return value here, and the
date/time and century writes are separate operations.

Matching Linux 6.18.58 sources are retained under `evidence/linux-6.18.58/`.
`generate_pm_trace()` uses unsigned32 SDBM hashing and the packed value
`user + 16 * (file + 997 * device)`. The RTC splits it across year, month, a
28-day day radix, hour and three-minute minute buckets. The clock then ticks.
Reading the kernel's later `Magic number` without accounting for that elapsed
time can produce a different file/device hash.

The offline Rust model has **five passing tests**, including all 16,095,568
valid user/file/device combinations, calendar rollovers, clock drift across a
three-minute bucket and unique one-day recovery windows for the 288 possible
action/user states when device hash is zero. The minimum encoded-date spacing
in that last set is four days. These are arithmetic tests, not a hardware
retention test. The model also handles year2000 explicitly; the kernel's
`tm_year > 100` adjustment has a year2000 edge case.

Zero device hash is a conditional assumption: it starts at zero and device
tracing updates it when `pm_trace_enabled` is true. The current retained history
has trace0 and no armed recorder. A future manifest must preserve and justify
that history instead of treating an unreadable internal variable as measured.
Decoding after recovery needs a bounded elapsed interval and an intact,
continuously ticking RTC. Firmware clock correction, CMOS clearing, loss of RTC
power, another writer, a failed marker write or unbounded recovery delay can
invalidate that inference.

The useful low-level boundary is:

- `acpi_suspend` start is emitted before `acpi_suspend_lowlevel()`.
- `acpi_suspend` end is emitted after a successful return and the
  `Low-level resume complete` message, before further ACPI/GPE/EC/NVS cleanup.

A retained start can narrow the last recorded region to S3 entry, power
transition or low-level return. It cannot distinguish those cases or identify
a driver. `suspend_enter` end is **before device suspension**, not the end of
resume. The final normal recorder marker is console_resume_all end.

## Older retained clues

Two preserved boot-RTC files from kernel6.18.54 contain early raw UTC clocks:

- Boot `3807011a-6769-45f3-91a4-35f290f3a57c`: `2099-07-02 15:05:20`.
- Boot `0e17a3fe-84fd-41d1-9dd8-7431dc1a2d62`: `2099-07-02 01:22:25`.

Under the zero-device-hash/continuous-clock/24-hour assumptions, each has one
candidate: action10, `acpi_suspend` start, value3. That marker encodes
`2099-07-02 00:00:00`; the two inferred elapsed intervals are54320 and4945
seconds. The kernel's naive Magic numbers instead read11:264:82 and11:86:356.
The upstream encoding sources for6.18.54 and6.18.58 are byte-identical, SHA256
`c60485b955de5d8d56c63714d15df932e9fc6637f4ad150cc236bc27465b918b`.

These files are compatible with retention across earlier recoveries. Their
reset provenance and old recorder binary identity are not fully established,
so they do not prove a failing phase on the current6.18.58 baseline. Selected
old requests also show differing raw-EC and booted/active generation states.
Do not merge those experiments with the present frozen baseline or infer that
all hangs share a cause. The old journals preceding these recovery boots have
already rotated out of the current journal inventory. The missing historical
STATE.md/PLAN.md remain missing; this assessment does not reconstruct them.

Two separately retrieved returning6.18.54 kernel captures show ordered deep
entry, low-level return and exit, plus the RTC-abuse warning. Neither contains
a `Timekeeping suspended` duration. Three retrieved after-phase files show
console_resume_all end with markers60 and an RTC near2023-04-10 00:00:01 UTC.
These support the overwrite/clock-side-effect interpretation, not a present
PASS or a measured sleep interval.

## Why this candidate did not pass preflight

The recorder destroys the frozen host's available sleep-duration clock:

1. `arch/x86/kernel/rtc.c::mach_get_cmos_time()` returns zero while the trace
   RTC-abuse flag is set. The persistent-clock path cannot measure sleep.
2. `drivers/rtc/rtc-cmos.c::cmos_read_time()` returns `-EIO` under the same flag.
   The RTC-class fallback cannot recover the missing interval either. It does
   not inject a huge date-derived interval; that read is rejected.
3. `kernel/time/clocksource.c` selects an independent suspend timing source only
   from clocks with `CLOCK_SOURCE_SUSPEND_NONSTOP`. Predator offers tsc, hpet
   and acpi_pm. Its CPU lacks `nonstop_tsc_s3`; the matching TSC source only sets
   that capability for this feature. HPET and ACPI-PM do not advertise it.
   Ordinary `nonstop_tsc` is insufficient.
4. Return markers rewrite the encoded clock before a userspace capture. The
   RTC observed after return therefore does not preserve the sleep interval.

A 960-second wake plan, the gap between Pi packets, or a later NTP clock step
cannot by itself prove at least900 seconds physically asleep. A separately
validated external-clock/monotonic estimator with quantified error and dated
physical evidence could be investigated, but none is established here. This
assessment does not declare such a method impossible; it refuses to substitute
an unvalidated estimate for the trial's measurement requirement.

There is also a writeback issue. `kernel/time/ntp.c` permits the kernel's
11-minute RTC writer while `STA_UNSYNC` is clear, without checking the trace
abuse flag. systemd262's timesyncd clears that bit during ordinary UTC clock
synchronization; its shutdown path does not explicitly set it. Stopping the
daemon alone is therefore insufficient proof that RTC writes are disabled.
A supported protocol must validate writer quiescence, preserve raw recovery
evidence before any clock restoration, restore the real clock/LAN/fresh NTP
safely, and handle failure while sleep is active. No such clock modification
was tested or applied to Predator during this assessment.

Boot preservation itself is deployed before timesyncd. On the current boot it
completed at18:26:54, before the first synchronization at18:26:54.284; the later
clock-handoff restored declared LAN/NTP/RTC. This is useful preservation
evidence, but cannot repair the missing sleep clock during an armed cycle.

The ordinary schema2 runner requires recorderoff; the diagnostic runner forbids
actual S3; the legacy recorder-enabled runner has obsolete kernel pins and lacks
the required current history/timing guards. None was repurposed. No new runtime
was deployed merely to bypass those refusals.

## Condition for the next test

Keep the current healthy boot and its full nine-cycle diagnostic history. The
next real-S3 attempt needs both a validated duration/physical-state measurement
and meaningful durable phase capture, with clock preservation/restoration and
bounded reset decoding explicitly tested. Independent timestamped power/phase
capture or a separately authorized recorder/kernel change could supply missing
evidence. The existing unsupported UART/pstore paths do not supply it by
assumption. No frozen module/kernel or security change is authorized here.

Once those prerequisites exist, prepare one distinct immutable instrumented
protocol, recheck full current health and actual cron timing, pin runtime and
history, prove the independent Pi capture/nonce/waker, and notify exact
entry/wake/review times before arming. Retain the standing one-test continuation,
08:00..23:00 window, measured sleep >=900 seconds, dated low-power graph and
owner-recovery-only handling of hangs. Do not schedule another blind S3 attempt
or a recurring preparation poll for this unchanged blocker.

`SHA256SUMS` covers this assessment, model, reference sources and selected raw
captures. Original historical files and prior trial/diagnostic archives remain
unchanged. Guarded remote synchronization receipts are kept separately in the
session scratch directory; no source or system build was needed for these
documentation and offline-model changes.
