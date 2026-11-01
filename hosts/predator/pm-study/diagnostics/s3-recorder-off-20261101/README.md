# Next discriminating S3 test: recorder-off control, November 1

## Armed at15:55 CET; result not yet observed

One recorder-off control is armed for **16:15 entry,16:20 RTC,16:21:30 Pi probe,16:23 frozen log and16:30 read-only review**. The preparation task deleted itself before machine actions. The separately known reviewer was verified enabled/due/current-thread before arming. See [sealed preparation](preparation/README.md), `schedule.json` execution data and the guarded synchronization receipt. Actual trial is `off-s3-20261101`; do not infer execution or physical sleep from these timers.

During preparation the owner clarified: **“same combined load. always the same thing. its always been the same thing. 3w correct suspend, 15w hang, 27w normal power on”**. This establishes a useful unchanged-meter/load comparison and corrects any suggestion that peripheral scope newly changed. The three values are owner-observed reference levels, not separately measured component powers. Keep all connections and powered loads unchanged; record16:13–16:30 and any button/lid action. The raw dated graph remains pending.

Once-only review collection is `.scratch/pm-recorder-off-20261101/operator collect .scratch/pm-recorder-off-20261101/off-s3-20261101 s3`; use existing primary/observer receipts if already present. Runtime/state/pins are in preparation. No new EFI phase record is expected or should be written. Changed boot or unsafe sleep permits bounded read-only evidence and notification only. After review, hand back without another power chain.

## Owner evidence and scope correction

After the14:20 trial, the owner confirmed: “its constant ~27w. schedule next est maybe ?” The approximately27W persisted throughout the discussed14:20–14:25 interval. In the scheduling clarification the owner said the meter includes **“Laptop and peripherals”**, and confirmed **“Available”** for local recovery and a power trace around16:15–16:30 CET.

This establishes sustained high consumption at the combined metered load. It does **not** establish that the laptop itself failed to enter hardware S3: separately powered drives/peripherals may account for part or most of it. The prior “S3 resumed successfully” phrasing overclaimed physical success; the subsequent assertion that27W alone disproved laptop sleep was also too strong without the meter boundary. The verified result remains successful software suspend/resume, three EFI boundary writes and cleanup; laptop low-power residency and exact wake cause remain unresolved. The raw dated graph has not been supplied. The sealed software review and earlier owner addendum are retained with their original scope.

## One justified control, not another baseline replay

Hypothesis: nonblocking EFI SetVariable calls at the final suspend/earliest-resume boundary may perturb firmware power handling even though the core control and software return succeed. The existing adapter makes these calls with IRQs disabled, one CPU online and SYSTEM_SUSPEND. The core test established callback functionality, not noninterference with actual S3. This is a testable hypothesis, not an established cause.

The single functional variable is **boundary recorder loaded/armed → not loaded, zero EFI writes**. Keep the last reviewed test's signed system/kernel/modules, security, storage and device configuration, pm_async0, pm_testnone, mem_sleepdeep, trace0, old RTC recorderN, normal clocks and RTC-only wake. No WoL. Retain Pi netconsole and bounded post-alarm reachability/frozen-log evidence. EFI space was6572 free/1452 above Linux's reserve after the last trial, below the old6912 admission threshold for another three-write sequence. Do not delete/reuse private records or lower that guard; this control needs no EFI writes.

Before deciding to arm, determine the actual power measurement boundary and record the attached storage/USB/PCI topology and independently powered loads. Prefer simultaneous or otherwise trustworthy laptop-only and combined-load traces. Ask the owner to meter the laptop PSU separately if feasible without disturbing data connections or storage power. Do not disconnect mounted drives, unmount/export pools, stop containers or manipulate drive standby merely to improve a graph. Do not assume USB bus-powered devices are excluded from the laptop PSU reading. If only aggregate power is available, retain that limitation and require a useful controlled comparison with the same attached loads; if the comparison cannot discriminate anything, leave awake and report why.

Predictions:

- A lower sustained laptop/controlled-load draw with the recorder absent supports an instrumentation-sensitive power path. It does not uniquely prove EFI writes caused the previous outcome; intermittency and peripheral state still matter.
- Persistent high **laptop-only** draw would show the EFI recorder is not required for the power-down problem and direct attention to platform/device power state.
- Low laptop-only draw with high combined draw would attribute the residual load to peripherals rather than establish a host S3 failure.
- A return or hang without a useful power trace is software evidence only. This deliberate recorder-off control cannot produce new EFI phase breadcrumbs; Pi silence after NIC suspend is expected and is not proof of physical sleep or a specific hang stage.

## Schedule and bounds

The exact scheduler receipts are in `schedule.json`. At scheduling time only the15:30 preparation and16:30 reviewer existed; machine timers were not armed then. Preparation subsequently armed the single control at15:55, with actual times and receipts above. The remaining instructions describe its bounded protocol.

All actions are November1, Europe/Paris08:00–23:00. Each task must first read this directory's `schedule.json` and delete only its stored exact task ID before machine action. Never search for its own task. Verify the separately known16:30 reviewer remains enabled and due before arming. Entry must be announced with more300seconds lead and occur no later than16:20, leaving snapshot+480seconds before16:30 review. RTC is entry+300, probe+390, frozen log+480. If preparation is late or conditions fail, do not replay or reschedule a power chain. The reviewer still reports the skipped/blocked outcome.

Owner availability is confirmed for16:15–16:30 only; do not silently move the experiment outside that window. Request a dated16:13–16:30 graph, meter/load description and any button/lid interaction. At most one new transition across these tasks, followed by read-only review and handback.

## Frozen baseline and new protocol requirements

Last reviewed boot`39d7579b-b86d-4d30-b910-131943e550c8`, btime estimate1793535079 with explicit logged±2s tolerance only for the estimate; exact boot UUID remains mandatory. Cumulative success2/fail0/oldRTCmarkers0: one core diagnostic and one real-S3 software return. Preserve both histories and the original core post-cleanup btime failure; no synthetic completion and no unchanged replay of old date/fresh-boot guards.

Frozen system`/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`, kernel82haph8j8hmbny68rzkis8dqrz9gqrr5 Linux6.18.58 and modules1qbmxim06wak3qplmj747hb5gkyzgj9z. Previous trial manifestSHA`bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c`, completionSHA`4e303475cf76fe2f972237f93ba15d18c5a77c6a52a9f6685e284eb1dbc96121`; original core-review anchorSHA`78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896`. Existing sources, state, EFI variables, GCroots and sealed archives stay intact.

Create a distinct Rust protocol/schema/state/manifest and pinned runtime explicitly for recorder-off, cumulative2→3. Record hypothesis, variable, unchanged device settings, power-measurement limits, predictions and owned rollback. Test rejection of changed boot/history, unexpected recorder/module activity, foreign alarm, unsafe cleanup and missing source timestamps. Pass focused tests, Nix build and applicable flake evaluation before deployment. Do not load any boundary/probe module or write EFI; no replacement instrumentation in the final suspend boundary. Retain the tested qzs clock helper, SHA`b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b`.

Fresh full preflight must verify all five authenticated boot/late-rejection audits, TPM CRB/Acer/raw-EC exclusions, HOLD absence, frozen identities/cumulative history, inactive sleep/MainPID0/no queued job, normal clocks/UTC RTC/NTP, AC/lid/display conditions, ordinary baseline modes and all boundary/probe modules absent, LAN/DNS/WoLg, healthy pools/five mounts/apps/four containers, and no maintenance/builders or conflicting timers. Recheck actual cron timing. A changed boot or uncertain prior cleanup blocks the trial.

Pi must independently prove its boot, synchronized NTP, active netconsole receiver, exact fresh nonce and frozen baseline before arming. Keep the canonical waker identity as provenance, but never execute/schedule WoL. Use bounded one-shot Typeexec/Restartno observers and primary with pinned owned cleanup. Capture absolute RTC write/readback and alarm IRQ before requesting sleep; refuse foreign alarms. Capture raw return before correction, wake IRQ/error, fixed/GPE69/IRQ/PCI/wakeup-source boundary data, and all four timestamp domains. Counter/register snapshots are not uniquely timed wake-cause proof.

Cleanup only owned ordinary PM settings/alarm after systemd-suspend inactive/MainPID0 and no queued sleep. Never kill active sleep or remove wake coverage; never redo completed cleanup. Unreachable/hung/changed-boot cases permit bounded evidence and owner notification only. Owner recovery must be recorded separately; no automated reboot/reset or old-boot restore.

Archive/checksum and synchronize only reviewed paths after fresh remote Git status, flake.lock inspection and exact preimages. No delegates, commits, pushes, activation, firmware/kernel/security changes, raw register writes, driver unbinds, thermal protection changes, storage power commands, calibration, extra wake, automatic retry or further power-test scheduling.
