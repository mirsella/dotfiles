# Clock-safe S3 boundary recording: preparation blocked

November 1, 2026, Europe/Paris. The owner authorized the proposed boundary-recording investigation and confirmed local recovery/power-graph availability. Later, after the EFI-capacity check, the owner separately selected **“Reboot once”**. That ordinary reboot completed; no new suspend, WoL, calibration or alternative-kernel test ran.

The required durable capture channel remains unvalidated. The probe's conservative capacity guard rejected module initialization before any variable write. A normal reboot did not increase available EFI storage. No S3 manifest, sleep timer or further power workflow was armed.

## What was checked

The frozen Linux 6.18.58 kernel has an active `efi_pstore` backend, but its backend advertises only `PSTORE_FLAGS_DMESG`. `CONFIG_PSTORE_CONSOLE`, `CONFIG_PSTORE_PMSG` and `CONFIG_PSTORE_FTRACE` are disabled. The backend's presence does not provide continuous suspend-boundary capture. Its 1,024-byte records and 10,240-byte kmsg budget also differ from the small explicit record considered here.

All four legacy UARTs report `uart:unknown`. ACPI DBG2/DBGP tables and ttyS device nodes do not establish a working external connection. No usable serial recorder was demonstrated. Ramoops is available as a module, but no reserved, reset-retaining buffer is established; a power cut can lose RAM contents. No firmware-reserved memory was repurposed.

Matching sources in `evidence/reference/` show that `efivar_set_variable_locked(..., true)` uses the nonblocking firmware path. Its kernel locks use trylock and its storage check does not request garbage collection. This avoids a kernel sleep/wait, but does not prove firmware latency, correctness with devices/syscore suspended, or retention through the owner's recovery method. The ordinary kmsg dump entry point is not an exported module interface; the EFI adapter uses the exported EFIVAR API directly.

The small C module is an adapter to that kernel API. It has no suspend hooks, RTC accesses, reset operation or automatic write. A unique absent private-GUID variable, explicit one-shot sysfs request and exact attribute/payload readback are required. The independent readback verifier is Rust. The module is built separately through `system.build.predator-pm-boundary-probe`; it is not added to automatic module loading or the booted system.

## Probe stopped before writing

At 11:33:22, the guarded loader attempted the pinned module with nonce `awake-20261101-a` and boot `f9b45aa6-b820-4ab5-b18c-9324d8d53087`. Initialization returned `ENOSPC`; `insmod` reported “No space left on device.” No sysfs write/readback request followed, and the module was not left loaded. The private GUID had no variable before or after this attempt.

Independent `statfs`/`df` observations at 11:34, 11:39 and immediately before reboot showed:

- Total EFI store: 163,752 bytes.
- Firmware-reported free space: 7,498 bytes.
- Linux reserved space: 5,120 bytes.
- Usable space above that reserve: **2,378 bytes**.

The adapter required at least **65,536 bytes of reported free space**. Thus this was a refusal by our deliberately conservative preparation guard, not a failed firmware write, proof that a small record could never fit, or proof of the S3 hang's cause. The guard was not weakened. Reset retention and suspended-context writes remain untested.

The metadata-only inventory contains firmware, boot and Secure Boot variables; it identifies no probe-owned record to reclaim. No variable values were collected by that inventory, and no EFI variables, boot entries or keys were deleted. No firmware garbage-collection request was made. The old RTC recorder remained off.

## One authorized ordinary reboot

The effective default and selected boot entry both resolved to generation 167:

`nixos-generation-167-o7mzz3pnl4dj6n4ajwtxp6rhpuc6k4pbdzfggpkl3govxscp665q.efi`

Its SHA-256 was `63a2619ff831156ba51c08c537efafa47a5c4c1e07ff5c52a7253435c7acdcd3`. A stale raw default-entry variable named generation 151, but `bootctl list` resolved the actual default to the current generation 167. The guard checked that resolved entry and image hash before reboot. No boot selection or EFI variable was changed by the controller.

At **11:58:50.479**, the controller saved its root0600 request receipt, synchronized evidence and issued exactly one ordinary `systemctl reboot`. The old journal records the request, normal shutdown and completion of the reboot target. Transient unit-start conflicts during shutdown remain in the raw journal; they were not treated as an S3 failure.

New boot: **`bf51c3ac-164e-4bfa-b56c-7b71df526b7f`**, btime **1793530757**, 11:59:17 CET. It was retrieved at 11:59:46 with expected startup jobs still running. The clock-handoff service completed successfully on its own. Full health at 12:02 and the additional 12:04 checks passed after startup completed.

EFI storage remained exactly 7,498 bytes free / 2,378 bytes usable. This reboot did not reclaim reported capacity. No second reboot or module-load retry was performed.

## Frozen identity and final health

- Current and booted system: `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`.
- Kernel: `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`.
- Modules: `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules`.
- All five authenticated boot and late-phase rejection audits passed; TPM CRB remained initialized, Acer WMI/raw-EC exclusions held, and study HOLD files were absent.
- New-boot suspend counters are **0/0**, markers0, recorderN, trace0, async1, pm_testnone, ordinary deep selection and empty alarm. Systemd-suspend is inactive/MainPID0; no jobs or failed units remain. The probe module is absent.
- LAN/DNS/WoLg and fresh NTP/UTC RTC passed; fast/tank are ONLINE with zero known errors and five expected mounts. Application services and all four Immich containers were healthy.
- The earlier scrub blocker cleared before this work: fast finished at10:10:39, tank at10:52:07, both with zero errors. No scrub was interrupted.
- Night-suspend and predator-pm-next timers remain inactive. No new power timer or scheduled review was created.

The previous boot retains its two successful s2idle cycles and unchanged October31/November1 manifest/completion hashes. Its kernel journal was copied before reboot. Those counts were not transferred to the new boot or erased from the historical interpretation. The new pre-NTP boot-RTC capture has SHA-256 `31f91f3ad3fb94342fa124fd3edfd8822df5275f86bec2e39a4285e6703989a8`; the reboot receipt has SHA-256 `bd97288f96ca81a03cf355889fa35597460f41a4ef0ce35146fc58b52a32f1f0`. Their original root0600 metadata is preserved in the tar archive.

## Build and evidence pins

Module package: `/nix/store/csz6rsq47hxsl16nbrrzqhadfgn41g9k-predator-pm-boundary-probe-20261101`.

- `boundary-probe.c`: `c379c8e9a9634a26f5a8e1b192e69281ff0491849b993f242c1402f5bb246097`.
- `boundary-probe.nix`: `7071bfd7b5f4c39ab1e69d7165adddd244032485089108dad5668b2862c9f021`.
- Built `pm_boundary_probe.ko.xz`: `9a10d09a4445282be5a8e026f7cfd298bfedb1cf68756b5ef9008dc3a40976cc`.
- Independent Rust verifier: `7efc9c539fa3cf5f82fdaeabbbdaf8c416fe2e51925922ed281816800ec803f6`.

The module Nix build, two verifier tests and `nix flake check . --no-build --no-update-lock-file` passed. Initial dependency-download/build and evaluation timeouts are preserved alongside successful foreground completions. A first read-only collector stopped on the absent optional `/var/lib/systemd/pstore` directory; its corrected result is separate. The successful probe build is not a successful hardware transport test.

`evidence/` contains source snapshots, pinned module export, matching references, exact scripts, failed and successful receipts, root-original archives, EFI metadata, old/new journals and health captures. `SHA256SUMS` covers this report and the archive. Existing GCroots and sealed October31/November1 reviews remain intact. No system activation, commit or push was performed; concurrent work was preserved.

## Resume conditions

This work is **blocked before the S3 experiment**. A verified external capture connection, or a separately justified firmware-storage/compact-recording approach with measured headroom, is required before claiming a durable recorder. A replacement must prove readback, retention and operation in the relevant suspended context while keeping RTC normal. A normal reboot alone did not provide that proof.

Once capture is established, the next experiment remains one attended, serial, RTC-only deep-S3 trial with a timestamped power trace and unchanged ordinary device settings. Its immutable protocol must explicitly account for this new boot and preserved history; old runtimes' boot/date guards must not be bypassed. Alternative-kernel or component comparisons remain conditional on the resulting boundary evidence. Nothing in this report schedules those actions.
