# Acceptance record

Updated 2026-10-08. The owner explicitly authorized full Predator deployment
after the earlier hold. No reboot or suspend test was requested or performed.

## Discovery and native package checks

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Target discovery | Read-only SSH inspection: NixOS Predator, working Caddy/HTTPS, root SSD state, existing suspend timer, no failed system units |
| PASS | Go account/model discovery | Direct authenticated models endpoint returned HTTP 200; configured `muse-spark-1.3-contributor` is present; no completion fallback attempted |
| PASS | Existing bot mapping | `getMe` identified `@mirsellabot`; `getWebhookInfo` had no webhook; `getChat` identified configured private owner `932980505` |
| PASS | Secrets provision | Encrypted separate environments; generated viewer login only in private `~/.config/hermes/viewer.json`; bot reuse preserved existing credentials |
| PASS | Native upstream Hermes messaging build | Official pinned messaging package built successfully; current configuration uses that unmodified derivation |
| PASS | Native Camoufox engine build and startup | Pinned official 152.0.4-beta.28 archive uses Firefox's relocation-preserving patchelf flags; bounded executable --version install check passes, and real headed launch on Predator succeeds |
| PASS | Native Camofox package build | Runtime-only locked npm dependencies and offline better-sqlite3 compiled successfully, before the final download-permission adapter adjustment |
| PASS | Earlier local full system build | `predator-hermes-ready-build` exited 0 at 00:17:39 before subsequent simplifications; not evidence that the current generation was activated |
| PASS | Simplified native Camofox build | Pinned package rebuilt successfully without the deliverables plugin |
| PASS | Upstream temporary-download API | Built package's real download listener/resource capture wrote into the selected temporary path with mode0640 under umask0077; explicit copy survived native session cleanup and uncopied sources were removed; mocked download input, no real Firefox or cross-user access test |
| PASS | Upstream persistence failure handling | Built native plugin wrote the supplied storage state, then rejected a forced write failure; final package imports both persistence/VNC plugins during build and includes their required shared cookie parser |

## Current stock Hermes setup

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Viewer/controller regressions | All 17 scoped browser tests pass, including ten-minute idle boundaries, fail-closed endpoint configuration, passive native tab adoption and cancellation-safe startup, plus authentication/assets/warm reuse/lost-tab/checkpoint/WebSocket checks |
| PASS | Managed instructions review | Workspace `AGENTS.md` requires normal stuck reply + viewer link, end turn, wait only for `done`, then fresh snapshot/verification; no browser polling |
| PASS | Native Telegram intake policy | Isolated fake-message probe against the pinned adapter: owner DM allowed, another user's DM and owner messages in groups/supergroups denied; no Telegram network call |
| PASS | Current Rust regressions | Exact deployed native Nix build ran 58 tests, zero failed/ignored; owner-default preservation, no-follow state adoption, worker-scope selection and nonrotating owner-secret provisioning join browser/restore/storage tests. Earlier scoped Cargo run passed all 12 Hermes tests; the changed browser scope passed all 17 tests |
| PASS | Owner configuration invariants | Gateway runs as existing mirsella with normal HOME, mutable runtime settings, owner SOPS secret, user bus and sudo-compatible service flags. Browser/controller hardening and private Telegram intake remain; real scheduled-worker delivery is still untested |
| PASS | Review scoped Nix build | Current host-tools and generated Hermes gateway unit/pre-start environment renderer build successfully; no activation or real-secret execution |
| PASS | Deterministic warm-path work measurement | 25 mocked-backend snapshots perform 50 backend requests, zero tab/VNC rediscovery and zero state-file replacements; inspected prior healthy warm path required 100 requests/50 replacements. This is work-count evidence, not real Firefox latency or power measurement |
| PASS | Stock-default Nix evaluation | All profiles and host/Predator invariants pass; verifies exact upstream messaging derivation, native local backend, no extra plugins/hooks/approval/toolset override, private Telegram and read-only download binding |
| PASS | Stock-default Nix package builds | Unmodified Hermes messaging package and current Rust host-tools built successfully; no activation |

No custom plugins, tool guard, terminal sandbox or cron source patches remain.
Terminal commands have the owner's personal home, credentials and passwordless
sudo access. A passing local check does not prove production
messaging/phone interaction.

## Predator deployment and runtime evidence

Activated 2026-10-08. Current generation:
`/nix/store/15p6rqc0bm1p2knlkbwka5bhjymi02dv-nixos-system-predator-26.11.20261001.c59305b`.
The pre-deployment generation was
`/nix/store/q0308gvyx73f72dlp04b1pywjjl7fqz2-nixos-system-predator-26.11.20261001.c59305b`.
No reboot or suspend test was performed.

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Initial full-system build and switch | Minimal viewer/restore source built successfully; copied closure matched target evaluation. Detached NixOS switch succeeded, followed by a successful declarative network-fix rebuild/switch at 15:06:41 Europe/Paris |
| PASS | Stock noVNC build and switch | Packaged noVNC 1.7.0 UI replaces the custom HTML/keyboard implementation. Final target build ran 51 tests and switched successfully at 16:14:46 Europe/Paris; gateway, controller and browser units active with zero restarts and no failed system units |
| PASS | Source preservation and pin | Original target sources saved on both machines; explicit reviewed source list synced without deletion. Target's newer nixpkgs `c59305b` retained; encrypted existing credentials matched and were preserved |
| PASS | Gateway startup and Telegram connection | Real SOPS pre-start environment loaded; all four integration units active. Telegram polling connected at 15:06:31 and reconnected at 15:10:34 after baseline backup; real owner DM/model/tool response remains pending |
| PASS | Private browser reply routing | Established/related browser replies remain allowed before browser-private-destination denial. The owner gateway now retains ordinary LAN access; the earlier agent-UID raw-API denial was intentionally removed during account migration |
| PASS | Deployed HTTPS viewer and authentication | Authenticated stock page, native UI/styles/icons/font/audio/package JSON and RFB/input assets return HTTP200. Page/assets without login return HTTP401; launch without CSRF returns HTTP403 and passive checks leave browser off. Earlier launch/WebSocket authentication, forged Origin and trusted-header checks remain valid for the unchanged authorization paths |
| PASS | Passive access and raw LAN ports | HTTPS probes leave browser off and viewer count zero. Raw 9377, 9378, 5900 and 6080 unreachable from main over LAN in the cold state; WAN/live-browser checks remain pending |
| PASS | Earlier custom phone UI, superseded | Prior icon-only UI passed local Unicode/IME/paste/focus/modifier/zoom/disconnect checks. That custom implementation has now been replaced by the upstream UI; these are historical results, not current native-phone acceptance |
| PASS | Stock noVNC UI, local fixture | Actual Rust controller serves native packaged HTML/assets with mocked RFB at mobile 390x844 and desktop 1280x900. Native Connect coalesces startup, keyboard focuses the editable input and sends text, extra keys send Tab, clipboard works, scale defaults to fit and Disconnect stays disconnected. Hostile URL host/port/path/autoconnect/reconnect settings cannot redirect or launch. Native startup failure restores retry; no console errors after correcting the mock's canceled-connect behavior. Physical phone/real VNC remains pending |
| PASS | Stock system light/dark theme | Small palette override preserves native layout. Light mobile and dark desktop backgrounds/margins checked; media-change handler updates the existing connected mock without reconnecting |
| PASS | Pre-migration backups | Original dedicated-account backup quiesced its user manager and published `hermes-20261008T131006.963800306Z.tar.gz`, directory 0700/archive 0600. A second consistent archive `hermes-20261008T144410.050812770Z.tar.gz` preceded owner migration; archives exclude personal-home account state |
| PASS | Encrypted off-machine baseline | Age-encrypted copy on main under `~/dev/hermes-backups-predator-20261008/`, using all three SOPS recipients; complete decrypt/authentication verification passed, with no plaintext archive written on main |
| PARTIAL | Restore | Disposable Nix/Cargo tests cover stale-file removal, modes/ownership, internal links, complete rollback and rejection of link escapes. Production restore/config reinstallation and ACL/xattr roundtrip remain untested |
| PARTIAL | Continuity | SSH, Caddy, Immich server, Nextcloud PHP-FPM and Postgres active; no failed system units after switch and backup. Existing HDDs/tank unavailable: backup/recovery mounts inactive, with writers mount-gated |
| PARTIAL | Browser-off overhead | At 15:15:35, six service-account PIDs total 475562 KiB PSS, about 464 MiB: gateway 255.7 MiB, Node 170.2 MiB, websockify 28.0 MiB, controller 5.3 MiB, user manager/helpers 5.2 MiB. No Firefox/Xvfb/x11vnc processes. CPU samples retained separately; full phase/idle-model verification pending |

The controller retried once during each cold backend startup, then remained
active. No ongoing restart loop was observed after the reply-routing fix.
Earlier idle samples at 15:08:30 and 15:10:06 covered 96 seconds: gateway CPU
increased by 1.861s, Camofox by 0.008756s, controller by 0.001390s and user manager
by 0s. These are cgroup CPU deltas, not power measurements. PSS and cgroup memory
are different measurements and were not added together.

## Real browser tool verification

The recorded Telegram Wikipedia test had two real HTTP502 failures. Passive
health had passed while the browser could not launch. The live checks below
were performed after fixing those failures through Nix.

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Native display and engine | Browser service declares which and gawk, points CAMOUFOX_INSTALL_DIR at the immutable version/resource bundle, and runs the engine patched with --no-clobber-old-sections. These fix missing display helpers/metadata and the native loader SIGSEGV; no runtime browser fetch or invisible fallback |
| PASS | Native adoption/startup contract | Authenticated GET /tabs is passive discovery and returns an empty list when cold. It no longer creates a blank tab that causes Hermes to skip its requested URL. A bounded startup task retains the lifecycle mutex across caller cancellation; regression tests cover both defects |
| PASS | Cold native navigation and snapshot | Actual running gateway environment and native Hermes tools opened Wikipedia Hermes in 9.24 seconds; success=true, a nonempty 500-element snapshot, shared home-browser identity and tracked tab 2c2336a8-b353-42f2-a5b3-d2089b1b6740 |
| PASS | Warm native interaction | Subsequent navigation retained the same tab. Native browser_type filled Athena, independently confirmed in the public page DOM; browser_press Enter reached the Athena article. A fresh reference click reached Wikipedia Main_Page, scroll/back succeeded, and a fresh snapshot returned to Athena without recreating the tab |
| PARTIAL | Wikipedia Search button | One native Search-button click returned success but submitted an empty query. Typing was independently verified and Enter submitted the same text correctly; the button-specific behavior was not repaired or claimed as passing |
| PASS | Real model uses browser tools | Native Hermes/Muse CLI session 20261008_201147_6f31b6 used browser_navigate and browser_snapshot, both success=true, and correctly answered that Athena is goddess of wisdom, warfare and handicraft. Durable state confirms those two tool calls; no terminal or web_extract workaround. This is a real model turn, not Telegram delivery acceptance |
| PASS | Live viewer transport | Authenticated same-origin controller WebSocket received real RFB 003.008 and security-handshake data; viewer accounting changed 0 to 1 while connected and returned to 0 after disconnect. This verifies the real VNC bridge, not physical-phone interaction |
| PASS | Final browser activation | Generation 6wx0h3s is active; gateway/controller/browser/Sleev/Secret Service all active, Result=success, NRestarts=0; no failed system units. Telegram polling reconnected at 20:05:27 Europe/Paris |

## Personal owner account and Sleev

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Owner account and retained state | Gateway User=mirsella, Group=hermes-private, HOME=/home/mirsella; existing /var/lib/hermes state adopted without following symlink targets. Native runtime configuration opts out of managed mode; missing-default seeding preserves edits |
| PASS | Actual sudo | From the running gateway's mount namespace, runuser as mirsella with the declared PATH executed sudo -n id -u and returned 0 |
| PASS | Pass | Independent Predator Hermes PAT, editor access to existing vaults, saved in Personal / Predator Hermes CLI; pass-cli info succeeds with the owner bus and dbus keyring context |
| PASS | GitHub | Native gh authentication uses the owner's existing token/SSH configuration; gh api user returns mirsella |
| PASS | Secret Service | Nix-owned GNOME Keyring service runs on the owner's user bus, unlocked with the SOPS password passed through stdin; Pass/GitHub use the working backend |
| PASS | Sleev native package and authentication | Pinned 1.8.8 CLI and inner gateway ELF/Python payload build/install checks pass. Independent Predator device login and auth verify succeed; immutable Nix system service is running |
| PASS | Independent OpenAI/Codex | Native hermes auth add completed device-code OAuth for label Predator at 17:23:53. No rotating OpenCode refresh token was cloned; pinned pool resolver honors HERMES_CODEX_BASE_URL |
| PASS | Provider routing and live catalogs | Native resolver selects intended provider/credentials and Sleev URL. Authenticated live catalogs return 38 Go models, 47 Zen models and 14 Codex models; Go/Muse remains the runtime default, no fallback selected |
| PASS | Go completion through Sleev | Muse Responses request through /sleev/hermes/opencode-go/responses returned status completed and nonempty output, using native session-affinity headers and normal OpenAI client User-Agent. A 32-token probe exhausted its reasoning budget; successful probe used 512, with 198 total tokens |
| PASS | Final owner activation | Detached NixOS switch completed at 17:56:19; all five gateway/browser/controller/Sleev/Secret Service units active with zero restarts and no failed system units |
| PASS | Owner-era live backup | Published validated hermes-20261008T155703.519033928Z.tar.gz; owner's user manager stayed active with unchanged PID 1292 across backup. Only integration units/native worker scopes are quiesced. All units recovered; controller retried once during cold backend startup, with no ongoing restart loop |
| PARTIAL | OpenAI API provider | /sleev/hermes/openai route configured, but no separate OPENAI_API_KEY exists. Independent ChatGPT/Codex OAuth is available instead |
| BLOCKED | Proton Mail | Native default-profile login reached manual CAPTCHA. Two coordinated challenges expired at 17:27:29 and 17:41:21 before completion; no authenticated mailbox session is claimed |

The first provider smoke requests used Python urllib's default User-Agent,
which upstream rejected with Cloudflare 1010. Using the native OpenAI client
User-Agent made the catalogs succeed. Go also requires its native
x-opencode-session header. These were probe defects, not credential rotation
or provider fallback.

## Browser access audit and ten-minute retention

Live checks used both LAN-resolved HTTPS and the public DNS hostname, with the
browser running. Public DNS was tested from main; this is not an independent
off-LAN phone or Internet vantage point. Credentials were passed privately,
never included in command arguments or audit output.

| Result | Check | Evidence |
| --- | --- | --- |
| PASS | Page and asset authentication | No login or wrong password returns HTTP401 with Basic challenge for page, status, JS/CSS/JSON/icons/fonts/audio; unauthenticated HEAD/OPTIONS and normalized path variants do not expose protected content |
| PASS | Actions and WebSocket | Unauthenticated launch/disconnect/WebSocket return 401; missing CSRF, forged action Origin and forged WebSocket Origin return 403. Forged trusted/forwarded headers do not bypass Caddy; authenticated trusted-header overwrite works |
| PASS | HTTPS and response policy | HTTP browser paths redirect to canonical HTTPS. Authenticated page has CSP, frame denial, no-store, nosniff and no-referrer; traversal and public API probes expose no private data. Passive requests leave runtime/viewer state unchanged |
| PASS | Live raw listeners | IPv4 browser API/controller/VNC/noVNC and Caddy admin are unreachable over LAN/public DNS. Unrelated local nobody is denied all five ports and IPv6 VNC. Privileged positive controls confirm the listeners are live, rather than merely closed |
| PASS | Authorized browser/viewer | Actual owner-UID native Hermes snapshot has 500 elements. Real TLS/Caddy/password/Origin/WebSocket connection receives RFB handshake data; viewer count changes 0 to 1 to 0 |
| PASS | Private state ownership | Live gateway state uses owner-only hermes-private membership, not shared users. Credential environment 0600, gateway SOPS secret 0400, controller/profile directories 0700; temporary download directory 2750 camofox:hermes-private |
| PASS | Cookie/localStorage restoration | Fixed the pinned Playwright request-interception schema mismatch without discarding saved profiles. A nonsecret cookie and localStorage sentinel survived actual checkpoint, browser stop and native tool restart; both test values were removed and cleaned state checkpointed |
| PASS | Retention and configuration | Deployed idle threshold is 600 seconds, checked at the 599-second boundary. Agent activity renews it; connected viewers suppress cleanup and disconnect starts a fresh grace period. Public listeners, remote backend/WebSocket URLs and non-HTTPS/path-bearing origins fail configuration validation. Full ten-minute process cleanup has not been timed live |
| PASS | Final activation | Generation 15p6rqc0 active; gateway/controller/browser/Caddy/Sleev/Secret Service active, Result=success, zero restarts and no failed system units |

Root and the owner account remain trusted administrators, including the
explicitly requested sudo access. The audit does not isolate the browser from
the owner. Browsers can cache Basic Auth credentials, so an already signed-in
browser may not show another password prompt.

## Production checks still required

The owner deferred the remaining interactive checks. Working startup, passive
HTTPS checks and local emulation do not prove these end-to-end behaviors.

| Check | Required evidence |
| --- | --- |
| Messaging real tool and denial | Owner DM executes a workspace tool; another user/group is ignored and cannot alter state |
| Reminder persistence/delivery | A temporary saved reminder survives gateway restart, delivers once to owner and is removed |
| Cold manual launch | Real native cold launch and VNC protocol readiness passed above; rendered native Connect/physical-phone interaction still pending |
| Warm session and race | Native warm navigation/input/click/scroll/back preserves the tracked tab; cancellation/coalescing regression tests pass. Remaining: live competing clients and prolonged manual viewer use |
| Authentication and raw ports | Live-browser auth/raw-port audit passed above. Remaining: independent off-LAN/mobile vantage point; public DNS was tested from main. Owner/root administrative access is intentional |
| Phone/mobile interaction | Real WAN/mobile phone login, viewport scaling, keyboard, click and disconnect; desktop emulation is insufficient |
| Instruction-based help | Main agent replies normally that it is stuck, links viewer, stops actions, then responds to `done` with fresh snapshot/verification; no ownership lock or custom handoff tool |
| Wait for done | No snapshots, browser requests, timer or background rechecks while waiting for the owner |
| Idle/restart during help | Task notes and native conversation retain blocker; agent reports lost challenge honestly and reopens saved URL; no custom goal budget reset |
| Former reaper threshold | Manual viewer use longer than five minutes remains live; no tab/page recreation |
| Real storage checkpoint | Actual checkpoint/browser stop/native restart cookie/localStorage roundtrip passed above. Full browser-service restart and IndexedDB remain untested; IndexedDB disabled |
| Explicit download retention | Stock local terminal reads but cannot write `workspace/downloads`; copied workspace file survives cleanup, uncopied source is removed, browser profiles stay private |
| Idle process cleanup | After ten minutes without tools/viewers, Firefox/Xvfb/x11vnc gone; retained watcher/websockify/control overhead measured |
| Bounded browser failures | Startup/display/backend failure is bounded and offers Retry; checkpoint failure retains browser; no unapproved provider/model fallback |
| Backup/restore | Remaining: production restore, Nix-config reinstallation and ACL/xattr verification; disposable replacement/rollback/link tests, owner-era backup preserving the user manager, pre-migration backups and encrypted off-machine baseline passed above |
| Continuity | Remaining: functional app/storage/power acceptance; service status passed above, HDD/tank backup mounts unavailable |
| Suspend/reboot | Pending explicit authorization and safe window; do not perform as part of this work |
| Resource and model-idle measurements | Timestamped PSS/cgroup CPU/process samples for the five documented phases; no double-counting, no inferred wattage, no idle model requests |

Use redacted evidence: timestamps/statuses/PIDs/resource values, never tokens,
authorization headers, cookies, passwords, profile databases or MFA contents.
