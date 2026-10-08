# Hermes and the shared browser on Predator

This integration is managed by the dotfiles flake and was activated on Predator
on 2026-10-08. The gateway is connected to Telegram. The acceptance record
distinguishes working services from the remaining end-to-end checks.

## Configuration

| Component | Pinned source or setting |
| --- | --- |
| Hermes | NousResearch/hermes-agent `865ba906c1a8d93de65839ee7af487204d42e873`, native Nix messaging package |
| Camofox | jo-inc/camofox-browser `39c82094013480b373df6600d44c7f036f58356e`, version 1.18.1 |
| Camoufox | Official Linux x86_64 engine 152.0.4-beta.28, patched ELF dependencies |
| Inference | Initial default OpenCode Go, `muse-spark-1.3-contributor`, through local Sleev |
| Sleev | Pinned native CLI/gateway 1.8.8, `127.0.0.1:17321` |
| Messaging | Existing `@mirsellabot`, numeric private owner `932980505`, outbound long polling |
| Viewer | `https://mirsella.mooo.com/browser/`, existing Caddy TLS and password authentication |
| Browser identity | `home-browser` user and session, one explicitly tracked shared task tab |
| Display | Private 1600x900 Xvfb, upstream x11vnc/websockify/noVNC |

`hosts/predator/default.nix` enables the integration and messaging. The upstream
Hermes module is imported only into Predator's host module. Neither desktop
gets these services. Initial model fallbacks are empty and auxiliary requests
use the same provider/model. Runtime model choices are editable; rebuilds add
missing defaults without replacing existing choices. External OAuth adoption is
disabled; Hermes has its own OpenAI/Codex login.

This follows the [official native NixOS setup](https://hermes-agent.nousresearch.com/docs/getting-started/nix-setup/),
using its documented smaller `messaging` package for Telegram. Hermes is
unmodified. Its toolsets, approvals,
memory, skills, delegation, goals and scheduler use upstream defaults. The
deployment supplies the initial model, workspace, private Telegram
policy and Camofox connection. Auxiliary model requests are pinned to the same
Go account/model. Browser-help instructions live in the managed workspace
`AGENTS.md`.

## Personal account and browser boundaries

The gateway runs as `mirsella`, with the owner's normal home, dotfiles, SSH keys,
LAN access and passwordless sudo. It can edit its runtime configuration and
deploy permanent host changes through `~/dev/dotfiles`. The browser and lifecycle
controller remain separate accounts, `camofox` and `hermes-browser-control`,
with their existing systemd restrictions and private state.
Hermes state and temporary downloads use the owner-only `hermes-private` group,
not the shared `users` group. Its generated credential environment is mode 0600.

Terminal and file tools use Hermes's documented default `local` backend under
`mirsella`. There is no custom terminal plugin, bubblewrap wrapper, tool guard
or patched scheduler. Temporary downloads are exposed read-only at
`workspace/downloads` through systemd's `BindReadOnlyPaths`; the agent can copy
selected files into its writable workspace.

A dedicated nftables output table blocks private/reserved IPv4 and local IPv6
destinations for the browser UID. Exceptions are the configured DNS resolvers
and websockify's local VNC transport. The owner's general terminal/network
access remains unrestricted.
Established replies are accepted so the private servers can answer their clients.
The table does not replace the host firewall. Raw API/noVNC ports bind to IPv4
loopback; native VNC also listens on IPv6 loopback. These ports are not opened
in the host firewall. Socket-UID rules additionally restrict the backend API
and noVNC to the controller, raw VNC to the browser account, and the controller
to Caddy/the owner. Root remains authorized on all of them. Caddy's loopback
admin API is restricted to root, Caddy and the owner too.

Caddy authenticates every `/browser/*` route, including WebSockets, then
overwrites the controller's private trusted header. Mutating viewer actions
require POST, exact Origin and a fresh per-process CSRF token. The controller
does not trust user-supplied proxy headers or forward arbitrary destination
URLs. The browser API has a closed route/identity/tab allowlist. The viewer has
no ownership lock: the main agent is instructed to stop when asking for help.
Owner-only DMs and disabled Telegram groups use upstream authorization policy.
HTTP viewer requests redirect to the canonical HTTPS URL. Controller startup
rejects public listeners, remote backend/transport addresses and non-HTTPS
viewer origins.

Authentication is existing-proxy Basic Auth, not SSO or a promised 30-day
remembered cookie. The browser may cache the login according to its own policy.
The owner's generated viewer login is in the private local file
`~/.config/hermes/viewer.json` on main and in Proton Pass, Personal / Predator
browser. Keep it out of messages and Git.

## State and secrets

| Path | Owner and purpose |
| --- | --- |
| `/var/lib/hermes/.hermes` | Owner-owned gateway databases, editable config, memory, skills, schedules and private `.env` |
| `/var/lib/hermes/workspace` | Approved agent files and task notes |
| `/var/lib/camofox` | Browser account, private profiles and engine cache |
| `/var/lib/camofox-downloads` | Temporary downloads, browser-owned and agent-readable |
| `/var/lib/hermes-browser-control` | Private tab/lifecycle record, without a duplicate cookie snapshot |
| `/var/lib/hermes-backups` | Root-only consistent manual backups |
| `/run/secrets/hermes_*_env` | Separate SOPS gateway/browser/controller/proxy environments |
| `secrets/hermes.yaml` | Encrypted source, existing age recipients |
| `~/.config/sleev`, `~/.local/share/sleev` | Owner's independent Sleev login, gateway config and database |
| `~/.local/share/keyrings` | Owner's persistent Secret Service for Pass, mail and GitHub |

State lives on the internal root SSD. No recurring Hermes job writes to the
HDD backup datasets. Cookies and localStorage persist through graceful stop;
IndexedDB persistence is deliberately disabled until a site needs it and it is
tested. Active DOM/challenges are not durable state.

Go and Zen keys are reused from the configured OpenCode accounts. The bot key and
owner are reused from the existing encrypted `TgToken`/`TgId` fields. Verification
found the expected bot, private chat and no webhook. Existing OpenCode/storage
notifications are outbound-only; they can share this bot. An additional inbound
poller must not be started.

Browser access, admin, cookie-import, agent gate and viewer keys are distinct.
The browser has no Go or Telegram credentials; the viewer receives no backend
keys. Native Hermes requires its private `.env`, readable by its own service
user and local terminal. The browser profile remains private to `camofox`.
Plaintext secrets never become Nix derivation inputs.
Predator installs SOPS through systemd. On every gateway start, the upstream
environment renderer runs after secret installation and refreshes Hermes's
private `.env`. The owner CLI sets `HERMES_HOME=/var/lib/hermes/.hermes` and
`HERMES_MANAGED=false`; the latter explicitly overrides the upstream managed
marker. Nix owns directory creation and ownership; the activation helper runs as
the owner and seeds only missing configuration keys without rewriting unchanged
files. Native scheduled workers use the owner's existing user bus.

The Nix-managed Secret Service unlocks with a separate SOPS password supplied
through stdin. Proton Pass uses an independent `Predator Hermes` PAT with editor
access to the existing vaults; its recovery login is Personal / Predator Hermes
CLI. GitHub uses the owner's existing token and SSH protocol. Proton Mail uses
the normal `default` profile. Account login state stays under the owner's home,
not in Nix derivations.

## Inference through Sleev

Hermes's native provider overrides point to these local routes:

| Provider | Base URL |
| --- | --- |
| OpenCode Zen (`opencode`) | `http://127.0.0.1:17321/sleev/hermes/opencode` |
| OpenCode Go (`opencode-go`) | `http://127.0.0.1:17321/sleev/hermes/opencode-go` |
| OpenAI API (`openai`) | `http://127.0.0.1:17321/sleev/hermes/openai` |
| OpenAI/Codex OAuth (`openai-codex`) | `http://127.0.0.1:17321/sleev/hermes/codex` |

The current default remains Go/Muse. The OpenAI login is an independent Hermes
OAuth grant; no rotating OpenCode/Codex refresh token was copied. The API route
is ready for a separately supplied `OPENAI_API_KEY` if needed.

Both Sleev executables are pinned and installed through Nix. The gateway release
contains an extracting launcher and an inner ELF/Python payload; the Nix build
extracts and patches that payload, then starts it directly. Runtime downloads or
a generic Linux loader are not required.

## Lifecycle and handoff

The Rust controller is the single cleanup authority. Passive page/status/health
requests do not start or renew the browser. Hermes's native GET `/tabs` adoption
probe also stays cold and returns an empty list when no browser is running.
Explicit authenticated POST or another real native tool request coalesces
startup and waits for `/vnc/status` readiness. The bounded startup task retains
its lifecycle lock even if the caller disconnects or times out, then hands the
same lock to the tool request. Ready state always carries a tracked tab.
Warm tool requests preserve the tracked tab and reuse validated tab/VNC
readiness while checking the live backend. Only tab identity is persisted;
monotonic in-memory activity avoids wall-clock jumps and per-tool state writes.
A controller restart starts a fresh ten-minute idle grace period. A native
missing-tab response invalidates discovery without replaying the failed action.
A lifecycle mutex prevents duplicate
launches and idle shutdown during a real HTTP operation; it is not a human/agent
ownership protocol. No task claims, leases, special handoff tool or resume RPCs.

Cleanup runs after ten minutes without real tools or connected viewers.
Stale viewers disconnect after failed pings. Housekeeping makes no model requests.
The main agent handles browser work. When blocked, its normal reply explains
the issue, links the viewer and ends the turn. The owner completes the step and
says `done`; the agent takes a fresh snapshot before continuing. Browser reads
remain technically possible while a viewer is connected, but the agent must
wait for `done`: no periodic snapshots, polling jobs or automatic rechecks.
Saved task notes and native goals retain context; no custom paused-goal database,
notification path, continuation parser or hidden goal-budget reset is added.

Cleanup requires a successful upstream storage checkpoint before closing the
browser. Downloads are temporary and upstream session cleanup may delete them;
there is no automatic copy/archive, deliverables plugin or extra download group.
Only files explicitly copied into the workspace are retained and backed up.
Firefox, Xvfb and x11vnc should disappear; the Node control server,
Rust controller, VNC watcher and websockify may remain. This residual overhead
must be measured on Predator after deployment, not presented as zero.

## Upstream compatibility patches

Camoufox uses the same relocation-preserving patchelf flags as Nixpkgs's
Firefox binary package. Its custom relocation loader must keep the original
section offsets; ordinary rewriting caused a startup SIGSEGV. A bounded
`--version` install check verifies the patched executable during the Nix build.
The service declares `which` and `gawk` for the native display/VNC helpers and
points `CAMOUFOX_INSTALL_DIR` at the immutable engine metadata/resource bundle.

The pinned Camofox server has no lazy-start switch and treats a zero tab timeout
as its default. Small fail-on-drift substitutions disable startup prewarming
and the registered-tab reaper, honor a zero timeout, refuse invisible headless
fallback and use Nix's noVNC path. The orphan-page reaper stays enabled. A small
download-path/mode substitution exposes only temporary downloaded files to the
agent's existing group; it leaves upstream download deletion unchanged.
The existing storage export route requires its persistence listener and
propagates failed writes, so idle cleanup can fail closed without a second
checkpoint implementation or cookie snapshot.
The pinned Playwright restoration path sends only the request-interception
fields supported by this Camoufox Juggler protocol. This keeps saved cookies
and localStorage restorable; an actual checkpoint/stop/start roundtrip passed.
Unpinned automatic add-on downloads are disabled; Camoufox's native
fingerprint behavior remains, but the optional default ad-blocker is absent.

Hermes itself has no source patches or custom plugins. Custom secret
provisioning, browser lifecycle/viewer and owner-operated backup commands are
compiled Rust in `pkgs/host-tools`. Custom operational programs remain Rust;
permanent services and timers belong in Nix and compile during the Nix build.
The default Hermes scheduler is used for ordinary reminders, with no seeded
monitor jobs or custom monitor executable.

The authenticated `/browser/` page uses the installed noVNC `vnc.html` and its
stock collapsible control bar, phone keyboard, extra keys, clipboard and scaling
settings. There is no copied toolbar or custom keyboard implementation.
`pkgs/host-tools/src/browser_viewer.js` replaces only the upstream bootstrap:
Connect sends an authenticated POST to start the browser before noVNC connects
to the fixed same-origin WebSocket. Autoconnect and automatic reconnect are
disabled, so page visits stay passive. `browser_viewer.css` adapts the upstream
palette to the system light/dark theme, including the display margins.
WebSocket connection, writes and close each have a ten-second deadline so a
stalled peer cannot retain a viewer indefinitely.

Hermes calls native Nix support best-effort. These revisions build natively;
there is no mutable FHS/container installation, runtime compiler or package
download. See [OPERATIONS.md](OPERATIONS.md) and [ACCEPTANCE.md](ACCEPTANCE.md)
for commands and pending production checks.
