# Compact EFI breadcrumb: awake write and readback, November 1

The owner approved a compact record with a measured allocation budget. One 64-byte record was written at12:40:49 CET and read back through firmware with the expected identity, phase, sequence, attributes and checksum. The independent Rust verifier passed. Reported free variable storage decreased by156 bytes, leaving7,342 free bytes: the Linux5,120-byte reserve plus2,222 usable bytes. The1,024-byte extra margin was preserved.

This proves the awake transport and its observed storage cost. Retention across reset and operation during suspended timekeeping remain untested. No suspend, reboot, WoL, variable deletion, garbage-collection request, system activation or new power schedule occurred in this step. The probe was unloaded; ordinary state and full health passed at12:48. The earlier normal reboot and failed64KiB-headroom probe remain in the sealed [boundary preparation](../s3-boundary-20261101/README.md).

## Record and budget

The private variable is `PMS3-20261101-a`, GUID `47d95147-3665-4bda-8ddf-d48f0e339023`, attributes7 (nonvolatile, boot-service and runtime access). It contains no full logs:

- Bytes0–7: `PMS3BND2`; little-endian version2 and length64 at8/10.
- Phase1, awake transport, at12; sequence1 at16; zero flags at20.
- Original boot UUID bytes at24: `bf51c3ac-164e-4bfa-b56c-7b71df526b7f`.
- Nonce `20261101-a`, NUL-padded, at40; zero reserved bytes at56.
- IEEE CRC32 over the first60 bytes at60, little-endian `0x5c258a74`.

The UTF16 name including terminator is32 bytes, so name plus data accounts for96 bytes before firmware overhead. The reported net storage decrease was156 bytes. Before/after total storage was163,752 bytes; free space7,498→7,342; firmware-reported maximum variable size7,438→7,282. Detailed logs and nine root0600 originals remain on disk, independently retrieved and hash-verified in `evidence/originals/` and the original ownership-preserving tar.

The one-shot module required Linux's reserve to equal5,120, at least1,024 additional margin plus1,024 write budget before attempting the operation, and a sufficient maximum variable size. It used the nonblocking Linux reserve-check and EFIVAR write path with local IRQs disabled, without the blocking path's garbage-collection trigger. Exactly one attempt occurred. The measured operation window was47,637,358ns (47.637358ms), including reserve checking and writing; this is not a proven latency bound for a suspend callback.

## Preserved failures and corrections

Firmware returned success (`write_errno=0`, `efi_status=0`) and exact readback. The deployed module then incorrectly required maximum variable size to remain unchanged. On this firmware it decreased with free capacity, so the module emitted `audit_errno=-116` and the sysfs caller received ESTALE. This was a post-write audit defect, not a failed write. Original status, error and outcome files remain unchanged.

The corrected audit checks stable total storage, no unexplained increase in free space, net consumption within1,024 bytes, the reserve/margin floor and sufficient remaining maximum size. It allows the maximum-size value to change. The independent verifier accepts the original ESTALE only in an explicit review mode, with a changed maximum and every payload, reserve, capacity and write-success check passing. It labels the result `ESTALE_DYNAMIC_MAX_REVIEWED`; other errors and capacity violations still fail. Its normal mode requires a successful module audit.

After unloading successfully, the original Rust operator's journal retrieval failed because it passed a hyphenated UUID to `journalctl -b`. The corrected read-only collector used the compact UUID and obtained the journal and originals. The operator source was corrected, but neither it nor the write was rerun. The original source, binary, pins and failure receipts are retained in `evidence/used-source/` and `run.*`.

The mounted efivarfs did not expose a new filename after direct kernel API creation. Direct firmware GetVariable supplied the successful readback. The missing cached filesystem entry is not treated as evidence that the firmware variable is absent; the filesystem was not remounted.

## Versions and checks

The deployed package was `/nix/store/3dzvzp3hg82n7gkrmrbac2svplh4xlzb-predator-pm-boundary-compact-20261101`; module SHA256 `39650a6e0654a280a775e3a75ad982539a4ebc60ba184389aad1f4e704be3e00`, C source `670442a614727e6bed14ecac534550d08a24a78f7919e4c72923cfc2610788c6`. Its permanent remote GCroot is retained.

The corrected, built package is `/nix/store/9piyqr32kaqa4c519f40v6mmhp7h6crr-predator-pm-boundary-compact-20261101`; module SHA256 `c8f956b93317b5f418b386359736e664c90395acad59536aa65d886db31fd6e4`, C source `696401f0719da3b239d54c9f2320f45c554270fbf9efa720df3b2a784bf0a19c`. It was built and retained locally, not loaded or used for another write. The Nix build attribute is separate and does not autoload the module.

Both module builds and flake evaluations passed. Four initial verifier tests passed before the live attempt; five tests passed after adding dynamic-maximum and explicit audit-review coverage. Tests reject corrupted/truncated records, wrong identities/attributes, invalid capacity accounting, reserve overrides, extra attempts, unknown audit errors and unexpected fields. `evidence/independent-verification.json` records the actual offline result. No package/runtime from the previous suspend trials was changed.

## Final state and remaining proof

At12:48:29–38, boot remained `bf51c3ac-164e-4bfa-b56c-7b71df526b7f`, btime1793530757, with frozen current/booted system `zbq64a1n4yzr3knpsgzhiw29lvgs3b34`, kernel `82haph8j8hmbny68rzkis8dqrz9gqrr5` Linux6.18.58 and modules `1qbmxim06wak3qplmj747hb5gkyzgj9z`. Suspend counters remained0/0, systemd-suspend inactive/MainPID0, no jobs/failed units, ordinary deep/none/async1/trace0, RTC recorderN/markers0 and empty alarm. Both probe modules were absent.

All five authenticated-boot/late-rejection audits passed. TPM CRB, Acer/raw-EC exclusions and HOLD checks passed; previous manifests, completion records, boot RTC captures and GCroots remained intact. Normal UTC RTC/NTP, declared LAN/DNS/WoL, both error-free pools, five dataset mounts, services and four healthy Immich containers were verified. Both scrubs had completed. The EFI disk partition remained unchanged with860,954,624 bytes free; no boot image was removed.

The next recorder validation must establish retained readback after an explicitly planned reset and suitability at the relevant suspend boundary, accounting for firmware-call latency and repeated-write storage consumption. The awake result alone does not establish either condition or diagnose the original15W S3 hang. A new immutable S3 protocol is still needed before another transition; no old boot/date/history guard was relaxed.
