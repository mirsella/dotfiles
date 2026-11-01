# Suspend phase-isolation diagnostics

Authorized October30 after the owner confirmed Q37 stayed around15 W and rebooted. Owner priority: "i just want each reboot i have to manually do have the most value possible, because im far away from it." Keep the standing instruction to schedule one useful next test automatically, but only after reviewing the preceding result. Reuse a healthy boot throughout this bounded ladder. A hang still requires owner recovery; no extra WoL, forced reset or unattended power cycling.

## Why this branch

Serial suspend hung at both37 minutes and12h25; asynchronous long-age suspend also hung. Long awake age and async execution are not necessary conditions. An earlier short serial attempt did resume, so intermittent behavior, state and reset differences remain. More age bisection lacks a reproducible passing bound. First identify how far the kernel can exercise suspend and unwind without entering firmware S3.

The existing6.18.58 kernel supports these pm_test checkpoints (Linux `kernel/power/suspend.c`):

| Level | Work exercised before the five-second unwind |
| --- | --- |
| freezer | Suspend preparation/notifiers and task freezing |
| devices | Adds platform begin, console and normal device prepare/suspend callbacks |
| platform | Adds late/noirq device and platform preparation callbacks |
| processors | Adds nonboot CPU offlining |
| core | Adds local IRQ/syscore operations, still skips actual firmware S3 entry |

A first failing level narrows a region, not an individual driver. A platform-level failure can involve late/noirq device callbacks, not just firmware. Repeated core returns leave actual S3 entry/resume untested. A last netconsole callback cannot locate a hang because the NIC carries that transport; the returningQ35 also stopped at the NIC boundary.

Plan: **freezer once, then devices/platform/processors/core twice each**, maximum9 verified returns in this initial ladder, one pending operation and one review at a time. Two returns are a reproducibility check, not proof an intermittent fault is absent. Do not skip a failing phase or automatically keep repeating hangs. After the ladder, reassess before selecting any new instrumentation/realS3 branch. Current kernel has genericpstore but lacksPSTORE_CONSOLE/PMSG/FTRACE; legacyUARTs are unknown,SPCR absent,DBG2 alone does not establish usable serial hardware. Continuous persistent capture requires further supported instrumentation, not an assumed working switch.

## Separate protocol and runner

`pkgs/predator-pm-study/diagnostic.rs`, binary `predator-pm-diagnostic`, package `/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0`.

- Binary SHA256`f1a3981959b6ea5eae30fa1e668b62cb15c7c21620529bfd3e777b59c68944d9`.
- Source SHA256`a5560085ebd86bf80a174d521146ea6bcefa20b0f1fba28262f3ac4fa6152bb2`.
- All31 tests passed: original24 plus7 diagnostic tests covering malformed/real/incomplete histories, delayed exit, missing journal/timeout, strict manifest/phase rules and unsafe restoration. Nix build passed. Imported package hash matched local/remote; no system activation. New source was indexed for Git-backed Nix evaluation, not committed.
- Root-owned0600 `pm-test-1` manifest pins boot/btime/system/kernel/modules/executable, date window, due, phase, ordinal and predecessor completed-fileSHA. `check|run|restore MANIFEST SHA256`; check requires>300s lead, run due..due+30, window08..23/≤12h containingdue+600. Record actualage; after first test there is no realS3 Q-age claim.
- State is only `/var/lib/predator-pm-diagnostic`. Create-new/fsync files. Never writes ordinary S3 last-completed. Ordinary schema2 source and its≥900-second requirement remain unchanged. Diagnostic completion is `DIAGNOSTIC_RETURN_VERIFIED`, `real_s3=false`, physicalsleepnot-applicable; neither3 W nor900s asleep is expected.
- Serial0,trace0,recorderoff,deep,delay5,TPMinitialized/rawECnot-run/inhibitors enforced. Ordinarypm_testnone/async1 before and after. Exact counters/history, no failures/markers, TPM/AC/lid/screen/WoL/LAN/NTP/storage/application/maintenance guards retained.
- Cursor-bound exact-boot journal capture requires one ordered deepentry→five-seconddebugmarker→exit plus systemdreturned/finished; rejects actual low-levelresume or timekeepingsleep. Reader deadline120s, childkill/reap/threadjoin recorded; separate rawobserved-journal andafter-kernel retained. Posthealth andcompletion≤due+180 required.
- **Never clear pm_test while systemd-suspend is active/activating, MainPID nonzero or a suspend/sleep job is queued.** Doing so could accidentally allow actualS3 when the operation reaches its checkpoint. Safe restore refuses and leaves selectedmode if inactivity cannot be proved. Do not override it to get a green check. Primarytimeout180s/ExecStopPost use this same safeguard.

## Review and continuation

Collect original diagnostic files and exactboot kernel/service journals, currentidentity/counters/modes/health, unchanged ordinarylast-completed, and full frozen Pi snapshot withsize/SHA. Classify missing retrieval separately from absent artifacts. No old S3 collector/classifier is used for this protocol. Physical observations remain useful for unexpected hangs but are not a physical-sleep criterion.

On verifiedreturn, pin actualcompletedSHA as predecessor, preserve sameboot/runtime and choose only same/immediatelynextphase. Stage a freshID/immutablemanifest, inspect actualcronforecast, use pinnedcheck with>300slead, then arm oneprimary plus Pi read-onlyprobe+120 andsnapshot+240. No WoL: the checkpoint should unwind itself. Review afterdue+300roundedup, self-delete/dateguard/verifiednextRunAt, propagate automaticcontinuation. Failedpreflight with no new cycle may retry under a freshID/appropriateunchangedordinal after clearing its cause. An unverified executed cycle cannot be synthesized into a completion to unlock the chain.

Current first operation: [d-freezer-01-20261030](d-freezer-01-20261030/README.md). No other phase is armed. Preserve system and both ordinary-runtime GCroots plus `.scratch/pm-phase-diagnostics-20261030/runtime-root`. No code/kernel/firmware/TPM/storage changes during the ladder, no commits/pushes, no unrelated edits or checkoutTMPDIR override.
