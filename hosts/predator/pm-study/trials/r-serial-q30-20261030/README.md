# Serial Q30 preparation, October 30, 2026

Status: **aborted before primary arming; no suspend executed**. This draft is preserved, not reused. The only actual pending test is [Q35](../r-serial-q35-20261030/README.md).

Schema-2 manifest `preparation/plan.conf`, SHA256 `97ba58c433bf52024f86a342647ee5fba1ba667d5546ee96d9bba19ebd4524cf`, selected profile Q, serial mode, ordinal 1 on boot `8b7a173d-8406-4072-aebc-c85c8b4989e7`. Its due time was 12:30:04 CET, exactly 1800 wall seconds after btime; wake was 12:46:04.

Stage succeeded. The first rehearsal refused an active Nextcloud cron job. That ordinary job completed successfully at 12:05:10, and independent inspection confirmed it inactive. The fresh rehearsal then passed. Arming established the observer timers, but final primary preflight calculated **1799 seconds**, below Q's 1800-second lower bound. The controller returned `cancellation=Ok(())`, with primary inactive and observer/notification timers canceled. No primary timer or requested artifact was created. Independent cancellation proof is in Q35's preparation archive.

All raw health, USB comparison, rehearsal, failed-arm and rollback evidence is retained under `preparation/`. No runner guard, immutable manifest or frozen baseline was changed. A fresh Q35 ID uses 2100 seconds, inside the supported 1800..2400 range.

The shared health inspection initially queried nonexistent docker/hermes-gateway/hermes-dashboard systemd units. Follow-up captures verify the real Podman and Hermes units active, along with all four healthy Immich containers. This was an inspection naming error, not an application outage or configuration change. USB topology and power-state snapshots matched the prior serial D byte-for-byte.
