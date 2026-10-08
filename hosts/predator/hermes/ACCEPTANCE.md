# Acceptance record

Updated 2026-10-08. This is a build-ready integration, not a claim of production
deployment. The owner explicitly deferred all Predator changes until after an
overnight auto-sleep test. No reboot or suspend test was requested or performed.

## Discovery and native package checks

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Target discovery | Read-only SSH inspection: NixOS Predator, working Caddy/HTTPS, root SSD state, existing suspend timer, no failed system units |
| PASS | Go account/model discovery | Direct authenticated models endpoint returned HTTP 200; configured `muse-spark-1.3-contributor` is present; no completion fallback attempted |
| PASS | Existing bot mapping | `getMe` identified `@mirsellabot`; `getWebhookInfo` had no webhook; `getChat` identified configured private owner `932980505` |
| PASS | Secrets provision | Encrypted separate environments; generated viewer login only in private `~/.config/hermes/viewer.json`; bot reuse preserved existing credentials |
| PASS | Native upstream Hermes messaging build | Official pinned messaging package built successfully; current configuration uses that unmodified derivation |
| PASS | Native Camoufox engine build | Pinned official 152.0.4-beta.28 archive and patched native dependencies built successfully |
| PASS | Native Camofox package build | Runtime-only locked npm dependencies and offline better-sqlite3 compiled successfully, before the final download-permission adapter adjustment |
| PASS | Earlier local full system build | `predator-hermes-ready-build` exited 0 at 00:17:39 before subsequent simplifications; not evidence that the current generation was activated |
| PASS | Simplified native Camofox build | Pinned package rebuilt successfully without the deliverables plugin |
| PASS | Upstream temporary-download API | Built package's real download listener/resource capture wrote into the selected temporary path with mode0640 under umask0077; explicit copy survived native session cleanup and uncopied sources were removed; mocked download input, no real Firefox or cross-user access test |
| PASS | Upstream persistence failure handling | Built native plugin wrote the supplied storage state, then rejected a forced write failure; final package imports both persistence/VNC plugins during build and includes their required shared cookie parser |

## Current stock Hermes setup

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Viewer/controller refactor | Nine controller tests retain authorization/identity/idle/persistence behavior and cover warm reuse, lost-tab recovery without replay, old-state migration and nonblocking passive status |
| PASS | Managed instructions review | Workspace `AGENTS.md` requires normal stuck reply + viewer link, end turn, wait only for `done`, then fresh snapshot/verification; no browser polling |
| PASS | Native Telegram intake policy | Isolated fake-message probe against the pinned adapter: owner DM allowed, another user's DM and owner messages in groups/supergroups denied; no Telegram network call |
| PASS | Current Rust regressions | `cargo test`: 40 passed, zero failed/ignored; includes backup service recovery after failed stop/archive and existing storage/suspend regressions |
| PASS | Deterministic warm-path work measurement | 25 mocked-backend snapshots perform 50 backend requests, zero tab/VNC rediscovery and zero state-file replacements; inspected prior healthy warm path required 100 requests/50 replacements. This is work-count evidence, not real Firefox latency or power measurement |
| PASS | Stock-default Nix evaluation | All profiles and host/Predator invariants pass; verifies exact upstream messaging derivation, native local backend, no extra plugins/hooks/approval/toolset override, private Telegram and read-only download binding |
| PASS | Stock-default Nix package builds | Unmodified Hermes messaging package and current Rust host-tools built successfully; no activation |

No custom plugins, tool guard, terminal sandbox or cron source patches remain.
Terminal commands intentionally have the service user's access to its own
credential/config files. A passing local check does not prove production
messaging/phone interaction.

## Production checks still required

All rows below are **NOT TESTED on Predator** until deployment is authorized.

| Check | Required evidence |
| --- | --- |
| Messaging real tool and denial | Owner DM executes a workspace tool; another user/group is ignored and cannot alter state |
| Reminder persistence/delivery | A temporary saved reminder survives gateway restart, delivers once to owner and is removed |
| Cold manual and cold agent launch | Authenticated POST/real tool starts a viewable browser; actual startup latency and `/vnc/status` readiness |
| Warm session and race | Existing PID/page/tracked tab preserved; competing cold starts create one session |
| Authentication and raw ports | Unauthenticated GET/POST/WebSocket rejected without launch; forged origins/CSRF rejected; raw ports unreachable from LAN/WAN and from the agent UID |
| Phone/mobile interaction | Real WAN/mobile phone login, viewport scaling, keyboard, click and disconnect; desktop emulation is insufficient |
| Instruction-based help | Main agent replies normally that it is stuck, links viewer, stops actions, then responds to `done` with fresh snapshot/verification; no ownership lock or custom handoff tool |
| Wait for done | No snapshots, browser requests, timer or background rechecks while waiting for the owner |
| Idle/restart during help | Task notes and native conversation retain blocker; agent reports lost challenge honestly and reopens saved URL; no custom goal budget reset |
| Former reaper threshold | Manual viewer use longer than five minutes remains live; no tab/page recreation |
| Real storage checkpoint | Cookies/localStorage survive actual graceful browser stop and full browser-service restart; IndexedDB remains untested/disabled |
| Explicit download retention | Stock local terminal reads but cannot write `workspace/downloads`; copied workspace file survives cleanup, uncopied source is removed, browser profiles stay private |
| Idle process cleanup | After five minutes without tools/viewers, Firefox/Xvfb/x11vnc gone; retained watcher/websockify/control overhead measured |
| Bounded browser failures | Startup/display/backend failure is bounded and offers Retry; checkpoint failure retains browser; no unapproved provider/model fallback |
| Backup/restore | Disposable consistent backup and restore, private modes, no path/symlink escape, managed-config reinstallation |
| Continuity | SSH, Nextcloud, Immich, Caddy sites, firewall, storage and power policy remain healthy after authorized switch |
| Suspend/reboot | Pending explicit authorization and safe window; do not perform as part of this work |
| Resource and model-idle measurements | Timestamped PSS/cgroup CPU/process samples for the five documented phases; no double-counting, no inferred wattage, no idle model requests |

Use redacted evidence: timestamps/statuses/PIDs/resource values, never tokens,
authorization headers, cookies, passwords, profile databases or MFA contents.
