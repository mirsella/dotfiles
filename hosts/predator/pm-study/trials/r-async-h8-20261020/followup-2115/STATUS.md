# Read-only follow-up, October 20 at 21:15 CEST

Predator remained unreachable through the explicit authenticated WAN route, SSH exit 255 with `No route to host`. Its boot, result, post-return counters, kernel journal, sleep duration, NTP and storage recovery state could not be observed. The owner had not supplied a power reading, full graph or attended-recovery confirmation in the thread.

Pi observations at 21:15:41:

- Receiver active; waker SHA256 still `ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8`.
- Frozen snapshot remains 1,201,689 bytes, SHA256 `d2c1ba165c52d45efaf9d7beb8e0583a5d1d09b722d7508bfc52c35a7b5eb8ed`, unchanged from the verified review copy.
- Live receiver log has the same size and a last modification time of 20:24:15.785389930. No new messages have been received since entry. The final callback remains the observation boundary, not a proven cause.
- LAN neighbor is FAILED. There are no future Pi study timers; historical notice-start timers are elapsed.

Classification remains NO_RETURN, suspected hang pending physical power evidence. A low-power sleep with failed wake has not been excluded. No compliant PASS or proven entry hang is claimed. Missing graph and missing reported wattage remain separate evidence gaps.

Original STATE.md and PLAN.md are still missing locally; they were not claimed to have been read or recreated. The recovery root and trial README were read. Raw previous captures and manifests were not changed. Current captures are `primary.*` and `observer.*` in this directory.

No reboot, reset, extra WoL, unlock, TPM/LUKS/keyslot/firmware change, new suspend, arm or preparation reboot was performed. No controller was invoked with the missing SSH shim. All SSH calls retained the instructed PATH prefix and explicitly pinned the documented WAN host and ports with strict host-key checks.

The spent 21:15 recurring task was deleted. The next and sole task is `pm H morning recovery`, October 21 at 09:30 CEST, nextRunAt `2026-10-21T07:30:00.000Z`, ID `scheduled-task:command:mcp:772ad742-9a60-495b-83d8-427b77074113:schedule-task:pm-h-morning-recovery-20261021-0930`. It is a fixed-time entry with an explicit date guard and self-deletion. The thread was unsnoozed. No overnight polling or real test is scheduled.

Next action is owner power/graph and local inspection or attended-recovery confirmation, followed by the dated read-only check. Only after actual H classification and recovery/health/access gates may a separate eligible D preparation be arranged. Never remotely recover a hung machine or arbitrarily reboot an eligible recovered boot.
