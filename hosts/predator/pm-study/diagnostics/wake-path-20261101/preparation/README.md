# First-slot preparation: path-d3-v2-20261101

Status at08:53 November1,2026 Europe/Paris: **armed, entry/result not yet observed**. One transition is scheduled at09:15. The exact08:30 task self-deleted before machine access. The enabled10:30 reviewer was verified at epoch1793519555 immediately before arming; no additional workflow was created.

## What this experiment tests

The October31 serial s2idle trial reached owner-observed3W and returned in the RTC-fallback window; earlier Pi WoL sends produced no recorded return. Its sealed evidence remains unchanged. The November1 awake checks established:

- `enp3s0f1`, MAC`98:29:a6:3a:a7:50`, is RTL8411b/r8169 at`0000:03:00.1`, under root port`0000:00:1c.3`. Both endpoint and ancestor wakeup were already enabled. The disabled ACPI`RP04.PXSX` belongs to sibling card reader`0000:03:00.0`, not this NIC. The NIC has no direct ACPI companion; root port maps to`\_SB_.PCI0.RP04`.
- Firmware `_OSC` explicitly withheld PCIePME ownership from Linux. No native PME IRQ was present; that agrees with the firmware-owned path and is not itself a defect. NIC and parent wereD0 at inspection; d3cold_allowed1 on all three devices. NIC awake PMCSR0x0008 has PMEenable/status clear; an awake reading does not prove sleep-time arming.
- At08:35:28.792258241CET a bounded Rust AF_PACKET receiver on the actual NIC captured the Pi's single labelled canonical WoL frame. It verified broadcast Ethernet, IPv4 source192.168.1.166/destination192.168.1.255, UDPport9, sixFF bytes and16 exact MAC repetitions. Full144-byte frame and matching sender receipt are retained. Predator was independently confirmed running with inactive sleep/MainPID0/counters1/0 before, immediately before and after that send. Pi usedwlan0; tcpdump was unavailable, so no sender pcap is claimed. This proves awake delivery, not receipt while asleep.

Matching Linux6.18.58 source in`source/` explains the discriminator. `pci_dev_check_d3cold()` denies upstream bridgeD3 when a child disallowsD3cold; `pci_bridge_d3_update()` propagates that decision. `pci_power_manageable()` treats a bridge according tobridge_d3, and the existing`pci_pm_suspend_noirq` debug site reports actual suspend state. `acpi_pci_wakeup()` walks upstream from a NIC without a valid ACPI wake companion rather than selecting the card-reader sibling. Native PME preparation and the successful ACPI wake path have existing debug sites.

The single functional control is **NIC`d3cold_allowed`1→0**, intended to keep its upstream bridge inD0 while the NIC still entersD3hot with PME armed. The runtime records original value1, verifies intervention0 and restores1 after safe return. Six original-off debug sites are enabled only for measurement and restored to their exact original-off flags:

```
drivers/pci/pci.c:2513 pci_pme_active
drivers/pci/pci-driver.c:923 pci_pm_suspend_noirq
drivers/pci/pci-driver.c:938 pci_pm_suspend_noirq
drivers/acpi/device_pm.c:956 acpi_pm_set_device_wakeup
drivers/acpi/device_pm.c:965 acpi_pm_set_device_wakeup
drivers/acpi/device_pm.c:971 acpi_pm_set_device_wakeup
```

A WoL-window return with verified parentD0/NICD3hot/nativePME/ACPI arming supports sensitivity to this power path under the instrumented condition. It does not uniquely prove the old bridge state or a firmware defect. RTC-only return with those states established shows this restriction insufficient. Missing or different state evidence means the mechanism was not established: the runtime preserves raw evidence and cleans up, then fails the measurement requirement rather than inventing the expected state. Logging is an added measurement condition, so a causal claim cannot ignore its possible timing effect.

## Pins, preflight and retained failure

The expected boot remains`f9b45aa6-b820-4ab5-b18c-9324d8d53087`, btime1793474520, cumulative success1/fail0/markers0. The new history guard validates the exact four October31 kernel phase source timestamps in both source domains and exactly its known BEGIN/RETURN, rejecting extra cycles or new trial BEGIN markers. The cursor-scoped new-cycle parser retains all four timestamp domains and permits genuine s2idle timekeeping suspension.

Full health08:41:29–39 passed: frozen current/booted system, kernel/modules; five authenticated boot/late-phase rejection audits; normal TPMCRB with AcerWMI/rawEC exclusions; ordinarydeep/none/async1/trace0/recorderN and empty alarm; AC/lid/framebuffer; LAN/DNS/NTP/UTC RTC; healthy fast/tank, all five mounts, application units and four healthy containers; no failed units/jobs/builders/maintenance. Runtime checks reasserted these conditions at08:51 preparation and08:52 arming, including exact topology, original settings, historical hashes, all six HOLD directories and original-off debug sites. Nextcloud cron last fired08:51:17 and completed08:51:18, next due08:56:17;09:15 is between its five-minute runs, with another strict quiet check immediately before entry.

Initial runtime`qar5vnhq2bk1vs3rr7g1z8kwa31bnwn4` passed tests/build but failed its live read-only snapshot because it used`gpe_14` instead of actual`gpe14`. It installed only the immutable initial manifest`path-d3-20261101-plan.conf`, SHA`d61d7b6686710bae2451f5a7525c5f19864d936d4bb56cb20704de3cf3c76ce3`; no settings changed and no transition was requested. Inspection and strace identified the filename; bothGPE names were corrected and read errors now include their paths. The failed source/runtime/manifest/receipts and GCroots remain preserved. The corrected trial uses a distinct ID and manifest, not an overwritten initial artifact.

- Schema:`s2idle-wake-path-20261101-first-1`; state`/var/lib/predator-pm-wake-path`.
- Runtime:`/nix/store/fgxkwdijv3z3n7iwkn7hv111xhydr8ax-predator-pm-study-0.1.0/bin/predator-pm-wake-path`; SHA256`3d655abf34d7a624ea6593c3db7c8ecf89e44930fb2640fcc85b54a9b61bc11c`.
- Source SHA256`afd512a3dfe31f75a4e6687a75c5b9ae9ce744b7eda28a17f619dbd158ddabbc`; unchanged timing SHA256`d84db2b41c3f89279938dcc959690d5f41e6a619848dfebeb3864ca31d9f4648`.
- Existing qzs clock-helper SHA256`b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b` remains pinned. Frozen system services were not activated or replaced.
- Corrected manifest SHA256`ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653`; remote root0600 path`/var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf`.
- Operator source SHA256`349864f8123ce74daa53c8bd20a1b34fd2c05e948f73b40939bf676dd3c5feee`; binary`53da8f89951ee96e96cf9d64bdd5735ab5c7fa92fb18a6358a23336e9ea49a3f`.
- Pi boot`2142644e-84db-40ae-b1b9-9e8c71265cf7`, NTP synchronized, normal leap/stratum2, offset−654µs; receiver active. Pinned wakerSHA`ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`.
- Independent Pi nonce`path-d3-v2-20261101-1793520900` reached netconsole; frozen baselineSHA`7a048b004674d029f7db85b196fedd4df07821bdeaab81d2863044143c0c72c2` matched the independently retrieved bytes.

All83 package tests passed before deployment; after the counter-name correction, all13 focused wake-path tests, a fresh Nix build and flake evaluation passed again. Both packet-decoder tests passed. Historical runtime source hashes were unchanged. Local and remote GCroots for both new runtime versions are retained. `runtime-v1.rs` preserves the initial source;`runtime-v2.rs`,`wake_timing.rs`,`Cargo.toml`,`Cargo.lock` and`default.nix` preserve final build inputs. Original pin files retain their original scratch/repository path names; the archive-wide`SHA256SUMS` uses paths relative to this directory.

## Armed schedule and safety conditions

All times are November1 CET. Owner notification and the09:13–09:40 dated power-graph request preceded arming. Primary arming completed around08:52:58, more22minutes before entry.

| Event | CET | Epoch |
|---|---|---|
| One s2idle entry |09:15:00|1793520900|
| Pi WoL |09:31:00|1793521860|
| Scheduled WoL retry |09:31:30|1793521890|
| Independent RTC fallback |09:35:00|1793522100|
| Bounded SSH banner probe |09:36:30|1793522190|
| Frozen Pi snapshot |09:38:00|1793522280|
| Existing independent review |10:30:00|1793525400|

Primary unit`predator-pm-path-path-d3-v2-20261101.service` isTypeexec, Restartno, RuntimeMax2400s, TimeoutStop180s, with pinned ExecStopPostrestore. Observer units are bounded30seconds, one-shot and nonpersistent. Arming receipts show primary inactive with no requested artifact yet. The absolute RTC alarm will be written and read back immediately before suspend; it is not claimed already armed at08:53. Failure to validate it prevents the suspend request. Serialasync0, s2idle/none, recorderN/trace0 and normal UTC/NTP remain the controlled protocol.

The runtime captures original/current PCI config, wakeup-source/IRQ/fixed-event/GPE counters, temperatures and raw RTC/realtime before correction. PCI reads are before/after, not live in-sleep snapshots; the existing source log establishes suspend states. Owned settings/alarm are restored only after systemd-suspend inactive/MainPID0 and no queued sleep. It never clears foreign alarms or debug flags, kills active sleep, or repeats completed cleanup. No deepS3, reboot, extra wake, watchdog, calibration, system activation, delegates, commits or pushes occurred.

## Independent10:30 review

1. Delete only the exact follow-up ID in`../schedule.json`, then enforce date/window and read the current parent README. Do not infer execution from the timer schedule.
2. Verify `.scratch/pm-wake-path-20261101-0830/operator-v2-pins.sha256`. Invoke the original operator **once**: `operator collect .scratch/pm-wake-path-20261101-0830/path-d3-v2-20261101 path-d3-v2-20261101`. Preserve collection failures, then independently retrieve originals and the Pi frozen result; compare SHA256 and root0600 metadata.
3. Inspect request/outcome/raw-return/restored/measurement/completed artifacts and unit result. Independently verify a single new s2idle cycle, four timestamp domains, exact old-cycle preservation, cumulative2/0/markers0 if successful, actual Pi sends, RTC expiry/IRQ/fixed-event deltas and actual parentD0/NICD3hot/PME/ACPI messages. Do not treat a WoL-window classification as proof of wake cause or a missing measurement/completed artifact as proof no return occurred.
4. Require restored NICd3cold1, all six original-off debug sites, ordinarydeep/none/async1/trace0/recorderN, owned alarm cleared/expired, inactive sleep/MainPID0/unqueued and full health/security/history. A completed cleanup must not be rerun. Unsafe/unreachable state permits bounded collection and owner notification only.
5. Apply the parent protocol's second-slot decision only after this independent review. A second trial needs a distinct tested/pinned protocol and new read-only review before arming. No unchanged repeat or third power-test workflow. This preparation archive remains immutable; put review evidence in a new directory.

The owner-confirmed October31 3W is valid context, but a new low-power level or physical duration for this experiment awaits the dated graph. Source-loop and receipt intervals alone cannot supply that duration.
