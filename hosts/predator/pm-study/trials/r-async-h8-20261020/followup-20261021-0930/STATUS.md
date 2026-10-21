# Read-only morning follow-up, October 21 at 09:30 CEST

The operational verdict remains HANG, explicitly confirmed by the owner on October 20. This check does not reopen that verdict or establish the exact failure phase. Numeric wattage, the full dated graph and recovery confirmation remain unavailable. The owner asked about battery depletion but did not report operating the plug or authorize agent recovery.

Fresh evidence:

- Predator's explicit authenticated WAN SSH route returned `No route to host`, exit 255. No remote commands executed there. Current boot, retained H result, journal, counters, actual sleep duration, NTP and storage recovery state remain inaccessible.
- Pi responded at October 21 09:31:02 CEST. Receiver active; waker SHA256 `ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`.
- Frozen snapshot remains 1,201,689 bytes, SHA256 `d2c1ba165c52d45efaf9d7beb8e0583a5d1d09b722d7508bfc52c35a7b5eb8ed`, matching the archived complete copy.
- Live receiver log remains 1,201,689 bytes with last modification time October 20 at 20:24:15.785389930. No new messages overnight. The final callback is not evidence of causation.
- Pi's LAN neighbor remains FAILED. There are no future Pi study timers, only historical elapsed notice timers.

The recovery README, trial README and owner report were read. Original local STATE.md and PLAN.md are still missing; they were not claimed to have been read or reconstructed. No prior captures or manifests were overwritten. This directory preserves the new `primary.*` and `observer.*` files; the complete earlier snapshot remains in `review-2051/`.

All SSH calls retained the instructed PATH prefix and explicitly selected the documented WAN host and ports with strict host-key checks, rather than relying on the absent shim. No controller, reboot, reset, plug operation, extra WoL, unlock, TPM/LUKS/keyslot/firmware change, suspend, arm or preparation reboot was performed. No new real trial is scheduled.

The spent morning task was deleted. The sole next task is `pm H afternoon recovery`, October 21 at 17:00 CEST, nextRunAt `2026-10-21T15:00:00.000Z`, ID `scheduled-task:command:mcp:dba95204-f8cd-4e90-a58b-ce72e8b3177e:schedule-task:pm-h-afternoon-recovery-20261021-1700`. It has an explicit date guard and self-deletion because fixed-time schedules recur. The thread was unsnoozed. No overnight polling is scheduled.

Owner recovery or inspection is still required before new preparation. If the owner reports recovery sooner, inspect and preserve the old trial artifacts, establish actual boot and health/access prerequisites, and arrange a separate immediate eligible D preparation without discarding an eligible recovered boot. Do not reset a hung machine remotely or shorten the D age to fit a time slot.
