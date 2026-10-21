# H verifier, October 20 at 19:24 CEST

Machine-side verification passed at 19:25 to 19:27. The existing arm and all wake coverage were left unchanged.

- Boot `3e3c970d-025f-4b78-8e00-f911e202e948`, boot epoch `1792491551`.
- Booted and current generation both `/nix/store/lzw3aij1nckap2iwdylhybla25kaz4nz-nixos-system-predator-26.11.20261001.c59305b`.
- Kernel and modules match the scheduled manifest. Pinned runtime and controller hashes match the preparation record.
- Remote manifest and the newly retrieved local copy `plan.conf` both have SHA256 `ef84699fe4007f6c9a749a045a2dabd8ecf015f48182e5c44164802c465fcd4e`.
- HOLD absent; suspend counters 0/0; phase markers 0; async 1; trace 0; deep selected; pm_test none. Current-boot kernel journal contains no suspend entry, low-level resume or exit signals.
- Exactly one primary study timer is waiting, due today at 20:24:11 CEST. No running attempt or jobs.
- Pi receiver active, waker hash matches. All five observer timers are waiting: notice-start 20:24:11, wake 20:40:11, retry 20:40:41, probe 20:41:41, snapshot 20:43:11.
- Existing `pm H review` is enabled for today at 20:51 CEST, nextRunAt `2026-10-20T18:51:00.000Z`, with the correct identity and manifest hash.

## Missing archive and transport

`/home/mirsella/dev/predator-pm-debug` and its SSH shim are missing on the laptop. STATE.md and PLAN.md could not be read there or at their corresponding paths on Predator and Pi. A narrow search of laptop dev directories and Trash did not find a replacement study directory. The original local manifest and historical captures were therefore not available for comparison or update. Their loss or relocation is unexplained; no deletion or migration was performed by this verifier.

The pinned local controller and Telegram environment file are still present. Credentials were not read or printed. The remote machine manifest, runtime, timers and Pi netconsole remain available.

Each SSH call retained the instructed PATH prefix but explicitly selected the documented WAN endpoint, because relying on the missing shim could contact an overlapping local LAN address:

- Predator: `/usr/bin/ssh -o HostName=mirsella.mooo.com -p 22 -o BatchMode=yes -o StrictHostKeyChecking=yes mirsella@192.168.1.19 ...`
- Pi: `/usr/bin/ssh -o HostName=mirsella.mooo.com -p 222 -o BatchMode=yes -o StrictHostKeyChecking=yes -l mirsella 192.168.1.166 ...`

Do not invoke the controller with only the missing shim PATH prefix. Its child SSH calls require a verified equivalent WAN mapping first. Otherwise collect read-only evidence directly using the explicit routes and arrange an evidence follow-up. Preserve the independent wake timers.

The thread was unsnoozed for the archive blocker. The review prompt was updated with this evidence location and transport issue; the spent verifier was deleted. The review remains enabled for October 20 at 20:51 CEST, nextRunAt `2026-10-20T18:51:00.000Z`. These task files remain in use by that review and must be retained until archived.

## Evidence

`primary.txt`, `observer.txt`, and their stderr files retain the machine observations. `plan.conf` is a fresh byte-exact retrieval of the remote manifest, not the unavailable original local file. `observer-documents.txt` records the missing Pi documents. `kernel-state.txt` records an initial journal query that rejected a dashed boot UUID; `kernel-signals.txt` contains the successful corrected query with the compact boot UUID and zero cycle signals. No machine configuration was changed by either query.

No reboot, rearm, cancellation, suspend, security change, build or test was performed by this verifier. Suspend remains planned for 20:24:11, wake for 20:40:11, and evidence review for 20:51 CEST.
