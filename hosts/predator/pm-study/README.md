# Predator suspend recovery evidence

Created October 20, 2026 because the original `/home/mirsella/dev/predator-pm-debug` directory and SSH shim were unavailable during the evening verifier and review. STATE.md and PLAN.md could not be read at their expected laptop, Predator or Pi paths. The cause or new location is unknown.

This directory preserves newly retrieved machine evidence and the retained October 20 verifier. It is not a reconstruction of the missing historical archive. Do not overwrite or remove existing captures.

Testing is paused at the owner's request on October 21, 2026. All thread-owned study schedules have been removed, including the October 22 recovery check. Do not schedule further checks, recovery operations or trials until the owner reports that someone has booted Predator and asks to resume.

Current trial: [r-async-h8-20261020](trials/r-async-h8-20261020/README.md), an owner-confirmed hang. The owner now reports that the battery reached 0% and a later WoL attempt did not boot it. Loss of the NIC's wake configuration is a possible explanation, not a verified diagnosis. Someone must boot Predator before testing can continue. No agent power or recovery operation was performed.

The recovered records are stored in the dotfiles repository at `hosts/predator/pm-study/`. The external recovery archive and retained scratch captures remain available; the unavailable original study archive has not been reconstructed. Captured reports describe their observation times and can contain superseded schedules. This README records the current paused state.

Use explicit authenticated WAN SSH routes while the original shim is missing. Merely prepending its absent directory to PATH does not route controller child processes safely. Keep the instructed PATH prefix, and use `-o HostName=mirsella.mooo.com -p 22` for Predator or `-p 222` for Pi, with BatchMode and StrictHostKeyChecking enabled. Do not silently contact the overlapping local LAN. No global SSH configuration or host-key trust changes were made.
