# Core recorder control reviewed, November 1, 13:54 CET

The core diagnostic returned on the same boot, both compact EFI writes succeeded, and restoration completed. Independent verification passed for all16 root0600 originals,1162 preceding kernel records, the final64-byte record, the bounded storage cost and both Pi frozen logs. This was `pm_test=core`, not real S3 or a physical low-power duration measurement.

The original harness subsequently failed its exact `/proc/stat` btime check. It has no completion artifact. That failure is preserved; `core-control-review.json` is a separate reviewed conclusion, not a replacement completion.

## Original result and guard failure

Trial`core-efi-20261101`, nonce`core1101a`, boot`39d7579b-b86d-4d30-b910-131943e550c8`, manifestSHA256`95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3`. Service started13:45:00.462985; BEGIN13:45:10.316756; deep diagnostic entry13:45:10.492030; core checkpoint and recorded phases arrived buffered at13:45:20.509; suspend exit13:45:20.637114. Cumulative success0→1, fail0, old RTC recorder markers0.

Original `outcome.txt` records operation/evidence/cleanup all `Ok(())`. The stage module unloaded13:45:21.850880. The clock helper obtained fresh NTP at13:45:24.622949. Afterwards, `/proc/stat` reported btime1793535079 rather than1793535078, while boot UUID and frozen system/kernel/modules were unchanged. The original exact-estimate identity guard then failed at13:45:26.012809, and ExecStopPost rejected the same estimate. Cleanup had already completed; it was not repeated.

The authenticated directory listing contains16 files and no `completed.json`, `verification.json` or original `after-kernel.jsonl`. Full kernel history was independently retrieved later. The standalone verifier explicitly requires this original failure/absence and checks original hashes rather than importing operational runtime code. The first read-only inspection stopped on an absent unloaded-module parameter; the corrected collector handles that state explicitly. An initial offline verifier assumption that restored.txt contained mem_sleep was corrected to use the actual collection and preserved as a verifier failure. Neither caused a new transition.

After proving cleanup and saving the unit's original status/journal, a pinned Rust helper cleared only this completed unit's systemd failure flag with `reset-failed`. It required that this was the only failed unit, MainPID0 and no sleep/job. It did not restore PM settings again or reboot. This housekeeping and its receipts are retained.

## EFI evidence

`stage-status.txt` records two attempts/two completions, finalphase11, disarmed, error0, results`0,0,-115`, EFIstatuses`0,0,0`. Exact final readback has attributes7,64 bytes, `PMS3BND2`, version2, length64, noncecore1101a, this boot, phase11/sequence2/flags7 and valid independent IEEE CRC32. Flags7 mean interrupts disabled, `SYSTEM_SUSPEND`, one CPU online.

Phase10 was written after all syscore suspend callbacks; phase11 before any syscore resume callback. Reported free space7342→7034, cost308 bytes, total163752 unchanged, maximum7282→6974. This satisfies Linux's5120-byte reserve, an extra1024-byte margin and256 bytes per attempted write. Both `fast_clock_delta_ns` values are0 because that clock was frozen; they are not latency measurements. The earlier awake record survived a separate planned ordinary reboot. This control's new record has not itself been tested through another reboot, and arbitrary power-loss retention is not claimed.

Raw return epoch1793537120/RTC1793537121 and timex8193 were coherent before helper correction. Restored async1/pm_testnone, ordinarydeep, trace0, old RTC recorderN/markers0, emptyalarm and unloaded stage were independently observed.

Four timestamp domains remain separate, in microseconds:

| Phase | Kernel source monotonic | Kernel source boottime | Receipt monotonic | Receipt realtime |
| --- | ---: | ---: | ---: | ---: |
| deep entry |2031857672|2031857672|2031913543|1793537110492030|
| phase10 begin |2033707416|2033707416|2037269554|1793537120509309|
| phase10 end |2033755209|2033755209|2037269592|1793537120509347|
| phase11 begin |2038741222|2038741222|2037269658|1793537120509413|
| phase11 end |2038785429|2038785429|2037269697|1793537120509453|
| suspend exit |2042222071|2042222071|2037397359|1793537120637114|

The kernel reported genuine timekeeping suspension4.661seconds. There was exactly one core diagnostic checkpoint and no low-level real-S3 return. None of these intervals proves physical low-power duration.

## Independent Pi evidence and health

BaselineSHA256`5c4218f7de7088a87b9fdc354e1e552d7c33e6313d9209f093542ac8e2055188`; resultSHA256`1f98c7583596a04a525373f7810cf0eed60b979674cc6ed59447b0400d75f077`. Independently retrieved baseline is an exact prefix of result. Its suffix contains one BEGIN and no RETURN, consistent with the later harness failure before its return marker. UDP netconsole stopped at the NIC callback and does not replay the disconnected interval. Pi SSH host key arrived13:48:00.981505, probe completed13:48:00.984925, frozen snapshot13:49:00.576856. No WoL was scheduled.

Fresh13:54:02–10 health passed: same frozen system/kernel/modules and boot, running/no jobs/failed units, counters1/0, ordinary PM/RTC state, all five authenticated boot and late-phase rejection audits, TPM CRB/Acer/raw-EC/HOLD checks, LAN1000/full/WoLg/declared UUID/DNS, synchronized UTC/RTC, both poolsONLINE/no errors, five dataset mounts, application services and four healthy Immich containers. Scrubs were complete; no active maintenance/builders. NTP server38.242.218.253 had offset−4.431ms/delay146.494ms/jitter27.915ms; this is retained as measured, not idealized.

The once-only collector already ran. The14:00 fallback task was deleted by exact stored ID before its due time because review was completed here. The separately pinned review anchor is SHA256`78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896`. A distinct S3-only runtime consumes it while preserving all original files and the original failed harness. No additional core cycle was needed.
