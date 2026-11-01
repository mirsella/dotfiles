# Freezer retry, October 30, 2026

**Armed, no result yet.** Fresh ID `d-freezer-02-20261030`, still diagnostic ordinal1/freezer with no predecessor. The previous attempt stopped before a PM cycle; its [review](../d-freezer-01-20261030/README.md) preserves the self-job guard failure. This retry keeps the same healthy owner-recovery boot `f8eb6e44-85ae-4e56-bc00-587876699945`, btime1793381192=18:26:32 CET. No reboot or WoL.

| Event | CET | Epoch |
| --- | --- | --- |
| Freezer diagnostic |19:23:32|1793384612|
| Pi read-only probe |19:25:32|1793384732|
| Pi full snapshot |19:27:32|1793384852|
| Review |19:29|1793384940|

Manifest `preparation/plan.conf`, SHA256 **91e1616b734ee77a0077bccd130cecfe5621dfa0fba8b2326207e1064a8f4484**, pins schema`pm-test-1`, freezer/ordinal1/predecessornone, serial0/trace0/recorderoff/deep/delay5/TPMinitialized/ECnot-run/inhibitors enforced, window18:00–23:00. Planned age3420s (57 minutes) is explicit diagnostic timing, not a real-S3 Q-age substitution. No900-second or3 W condition applies to a deliberately unwound diagnostic; no ordinary S3 completion/primer is created.

System `/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363`, kernel `/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage`, modules `/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules` remain unchanged. Diagnostic package `/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0`, binarySHAf1a3981959b6ea5eae30fa1e668b62cb15c7c21620529bfd3e777b59c68944d9 and sourceSHAa5560085ebd86bf80a174d521146ea6bcefa20b0f1fba28262f3ac4fa6152bb2 are unchanged. Existing31 tests/build remain valid; no runtime edit or system activation.

## Explicit orchestration correction

The first primary used Type=oneshot, so its own start job made the strict empty-job check fail. Harmless paired rehearsals reproduced that behavior and showed an empty job list under Type=exec. This primary uses **Type=exec, RuntimeMaxSec=180s, TimeoutStartSec=30s, TimeoutStopSec=30s**, plus the same pinned run and safe-restore ExecStopPost. An explicit runtime limit is required because Type=exec's start timeout does not cover its running lifetime. No guard is ignored or weakened. Only the archived first attempt's explained failed-unit state was cleared.

Stage/check at19:11:36 and final arm check at19:12:26 passed on the expected boot with counters0/0, markers0, recorderN, pm_testnone/async1 and no requested artifact. Source/binary/kernel remained pinned. Actual cron ran19:11:38.094268 to19:11:39.486795; its five-minute/one-minute-accuracy phase projects before19:23:32, but runtime maintenance guards remain authoritative. No timer configuration was changed.

The Pi received expected-boot capture marker nonce1793383960. Full frozen baseline **2,456,189bytes**, SHA256 **391a7d6f5e41599e3b7cc609e6a8ed952e9300ec279ea14f71a657cce786d90e**. Only read-only probe/snapshot accompany this test. Independent19:14 proof confirmed primarywaiting/serviceinactive, Typeexec/runtime180/start-stop30, counters0/0, ordinary modes, no jobs/failedunits/requested artifact, NTPyes, and matching observer timers. Notice **88814**, epoch1793384060, acknowledged the prior abort and new schedule.

## Review and continuation

Exact task `scheduled-task:command:mcp:7c78a459-384e-4141-9c94-d8ac1cb43ff2:schedule-task:predator-freezer-retry-review-20261030-1929` is enabled, bound to the current thread, with verified nextRunAt **2026-10-30T18:29:00.000Z**. Delete it at start even stale/early; machine actions onlyOctober30 from19:29 to before23:00. Preserve all old captures and use fresh paths.

Collect original root diagnostic artifacts, full expected-boot kernel/service journals, unchanged ordinary last-completed and full frozen Pi result `.agents-local/d-freezer-02-20261030-result.log`. Require the ordered five-second diagnostic cycle, successful logind/service return, reader cleanup, operation/cleanupOk, counters1/0/markers0, restored modes, healthy clock/storage/apps and a valid `DIAGNOSTIC_RETURN_VERIFIED` completion. Never synthesize one. Never clear pm_test during an active/queued sleep operation or override safe-restoration refusal.

Only after a verified freezer return, automatically prepare one devices test on this same boot, ordinal2, pinning this test's actual completed-fileSHA. Preserve Typeexec/runtime-limit correction. Follow the [bounded ladder](../README.md), review every operation, and keep one pending test. A hang requires owner recovery; no extra wake/reset or polling replacement. A zero-cycle guard refusal may retry only after its cause is understood and cleared. This setup gives no freezer pass or hang result yet.
