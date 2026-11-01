# One RTC-instrumented S3 trial, October 31

**Armed, outcome pending.** The owner authorized “do all then. test it”. The
preceding recorder-enabled core calibration completed09:35:19 and independently
verified09:41–09:44. One real S3 trial was then armed09:45:49–09:45:51 CET,
without a reboot or baseline change.

## Schedule and lifecycle

- Entry: **09:55 CET**, epoch1793436900.
- Pi WoL: **10:11**, epoch1793437860; retry10:11:30.
- Bounded LAN probe:10:12:30; frozen netconsole snapshot:10:14.
- Automatic review: **10:21**, verified nextRunAt`2026-10-31T09:21:00.000Z`.
- Exact review task:
  `scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-rtc-s3-review-20261031-1021`.
  It reads the retained scheduling record, deletes that exact ID at entry and
  enforces October31,08:00–23:00 Europe/Paris.

The original conditional09:50 target was not armed. More than548seconds of lead
remained when arming finished. The primary was inactive, no requested artifact
existed, and its timer was waiting09:55. Type=exec, RuntimeMaxSec2400,
TimeoutStopSec180 and the exact pinned safe ExecStopPost restore were verified.
Pi units have explicit bounded execution and nonpersistent, one-second-accuracy
timers. Wake coverage is independent of Predator's RTC.

Actual Nextcloud cron ran09:42:37–09:42:38; next timer09:47:35 projects the next
activation09:52:35, before09:55. Runtime maintenance exclusion remains mandatory;
no cron setting changed. Owner notification was acknowledged before arming and
requested the dated **09:53–10:15** power graph.

## Immutable plan and history

Manifest `/var/lib/predator-pm-instrumented/rtc-s3-20261031-plan.conf`, root0600,
SHA256`aa5be693e84beca952edddf18a7594169c3b08e3885bf61caaf4ca68d1ba5fa9`.

- Boot:`f8eb6e44-85ae-4e56-bc00-587876699945`.
- Before trial: ten diagnostic cycles, zero actual S3 attempts, counters10/0,
  cumulative markers56. The original nine diagnostics retain their meanings;
  the tenth is the separately verified RTC core calibration.
- Observed btime1793381198; original1793381192 and pre-calibration1793381193
  retained. Planned age since the original boot is55708seconds,15h28m28s. This
  is not a fresh Q trial or an ordinary D/E primer.
- Owner recovery mechanism and off duration remain unspecified.
- Calibration predecessor completion SHA256:
  `2333c0af7c6fa351a53ad5202189df7b469963590245485cb386ae7f6b6227ed`.
- Runtime:
  `/nix/store/jzmh15h5pf69b7xcz6g61xm2584f4ybp-predator-pm-study-0.1.0/bin/predator-pm-instrumented`.
- Binary SHA256:
  `31b46685045189ce6f8e73df2aaafb7944d86a5ff6c3d3f3782aaab4f58655a1`.
- Source SHA256:
  `947f45892c8c052553938d4df5e7dbfb36db83d367e7ffde76e14db4156dea87`.

The frozen system, kernel6.18.58, modules and RTC recorder remain unchanged.
TPM initialized/CRB, rawEC not-run and Acer exclusions remain mandatory.
The protocol selects serialasync0, trace0 and recorderRTC-unsync-zerooffset;
ordinary none/async1/trace0/recorderN must be restored after a genuine return.

## Independent capture and eligibility

Full09:42 health passed all five authenticated boot/late-phase rejection audits,
AC/lid/fb/WoL, declared LAN/DNS, fresh NTP, both pools/five exact mounts and apps,
without jobs, failed units, HOLD, builders or active maintenance. Pinned checks
ran again at preparation09:44:37 and primary arming09:45:50.

At09:44:39, Pi boot`2142644e-84db-40ae-b1b9-9e8c71265cf7`, synchronized NTP and
the active netconsole receiver were independently checked. NTP offset−563µs,
delay9.279ms, normal leap, packetcount592. The exact nonce
`rtc-s3-20261031-1793436900` was in its live capture. Frozen baseline SHA256:
`d9acaf48a67f432635116320aa1dae036f03edf218e788605fcee8508a246b40`.
Waker `/home/mirsella/wol_predator.py` SHA256:
`ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`.
Receiver, synchronization, waker and deadline were checked again before Pi
timers were armed09:45:49.

## Interpretation and recovery

A successful technical return is `INSTRUMENTED_RETURN_PENDING_POWER_PROOF`.
Full PASS requires requestOk, ordered genuine entry/low-level return/exit, valid
timing, restored full health and this trial's dated graph measuring at least900s
of physical low-power sleep. Scheduled960s, NTP correction and unreachability
are not physical-duration measurements. No graph or S3 outcome is available at
arming.

On a hang, collect bounded independent evidence and request owner recovery.
No recovery WoL, reset, watchdog, crash trigger, polling or blind repeat. Preserve
the next boot's early RTC capture before interpreting its marker under the
documented elapsed-time/device-hash/firmware assumptions. The last netconsole
NIC line cannot identify a culprit. Never clear pm_test or force restoration
while sleep is active or queued, or cancel executing primary/wake coverage.

Preparation evidence is in `evidence/`; original calibration review is alongside
this trial. Earlier runtime roots, failed preparations and histories remain
unchanged. No delegates, commits, pushes or system activation were used.
