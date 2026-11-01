# Owner reports approximately27W during the S3 trial

Recorded2026-11-01T14:49:29+01:00, after the sealed14:30 review was synchronized. The owner said:

> the power graph is ~27w, it didn't sleep i think ?

This observation concerns`s3-efi-v2-20261101`, the14:20–14:25 RTC-only attempt. It changes the physical interpretation: **software suspend/resume returned, but useful low-power sleep was not demonstrated; the owner reports high power.** The earlier wording “S3 resumed successfully” was too broad. Kernel `Low-level resume complete`, successful EFI phase20/21/22 records, suspend counters and normal cleanup prove progress through the software path, not sustained hardware low power.

The approximately27W reading is accepted as the owner's observation. The underlying dated graph/samples have not been provided, and whether27W persisted throughout14:20–14:25 or was a transient is still unconfirmed. If sustained throughout that interval, it means the intended low-power outcome failed. These data alone do not distinguish failure to enter/stay in hardware S3 from substantial components remaining powered. Neither the previous15W hang nor the earlier3W/6W s2idle observations are relabelled or treated as controlled power comparisons.

The same-boot software return, all21 original artifacts, three EFI writes, cleanup and14:31 health remain independently verified. There was no non-returning software stall in this attempt. The physical high-power symptom and its cause remain unresolved. Exact wake attribution and owner button/lid interaction also remain unconfirmed.

The [sealed review](../review-1430/README.md) and its original `physicalSleepSeconds:null` / `fullPhysicalPass:false` result remain intact. This dated addendum corrects the interpretation without rewriting evidence. It records no new machine observation, power operation or test authorization. Initially recorded locally; any remote synchronization requires fresh status/flake-lock/preimage checks.
