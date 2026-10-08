# Hermes and the shared browser on Predator

This integration is prepared in the dotfiles flake. It has **not been activated
on Predator**. Deployment is deferred until after the owner's overnight
auto-sleep test. Do not switch generations or change the running host as part
of the remaining local checks.

## Configuration

| Component | Pinned source or setting |
| --- | --- |
| Hermes | NousResearch/hermes-agent `865ba906c1a8d93de65839ee7af487204d42e873`, native Nix messaging package |
| Camofox | jo-inc/camofox-browser `39c82094013480b373df6600d44c7f036f58356e`, version 1.18.1 |
| Camoufox | Official Linux x86_64 engine 152.0.4-beta.28, patched ELF dependencies |
| Inference | OpenCode Go, `muse-spark-1.3-contributor`, `https://opencode.ai/zen/go/v1` |
| Messaging | Existing `@mirsellabot`, numeric private owner `932980505`, outbound long polling |
| Viewer | `https://mirsella.mooo.com/browser/`, existing Caddy TLS and password authentication |
| Browser identity | `home-browser` user and session, one explicitly tracked shared task tab |
| Display | Private 1600x900 Xvfb, upstream x11vnc/websockify/noVNC |

`hosts/predator/default.nix` enables the integration and messaging. The upstream
Hermes module is imported only into Predator's host module. Neither desktop
gets these services. Model fallbacks are empty, auxiliary requests use the same
provider/model, and external account adoption is disabled.

This follows the [official native NixOS setup](https://hermes-agent.nousresearch.com/docs/getting-started/nix-setup/),
using its documented smaller `messaging` package for Telegram. Hermes is
unmodified. Its toolsets, approvals,
memory, skills, delegation, goals and scheduler use upstream defaults. The
deployment supplies only the selected model, workspace, private Telegram
policy and Camofox connection. Auxiliary model requests are pinned to the same
Go account/model. Browser-help instructions live in the managed workspace
`AGENTS.md`.

## Security boundaries

Three service accounts separate the gateway, browser and lifecycle controller:
`hermes-agent`, `camofox`, and `hermes-browser-control`. None receives sudo,
personal SSH keys, a Docker socket or the owner's home. All services use
`NoNewPrivileges`, `ProtectHome`, a read-only system and explicit state writes.

Terminal and file tools use Hermes's documented default `local` backend under
`hermes-agent`. There is no custom terminal plugin, bubblewrap wrapper, tool
guard or patched scheduler. The service account and systemd restrictions
protect the owner's account; commands can read the agent account's own
token/config files. Temporary downloads are exposed read-only at
`workspace/downloads` through systemd's `BindReadOnlyPaths`; the agent can copy
selected files into its writable workspace.

A dedicated nftables output table blocks private/reserved IPv4 and local IPv6
destinations for the agent/browser UIDs. Exceptions are the configured DNS
resolvers, the agent's authenticated gate, and websockify's local VNC transport.
The table does not replace the host firewall. Raw API, VNC and noVNC ports bind
only to IPv4 loopback and are not opened in the firewall.

Caddy authenticates every `/browser/*` route, including WebSockets, then
overwrites the controller's private trusted header. Mutating viewer actions
require POST, exact Origin and a fresh per-process CSRF token. The controller
does not trust user-supplied proxy headers or forward arbitrary destination
URLs. The browser API has a closed route/identity/tab allowlist. The viewer has
no ownership lock: the main agent is instructed to stop when asking for help.
Owner-only DMs and disabled Telegram groups use upstream authorization policy.

Authentication is existing-proxy Basic Auth, not SSO or a promised 30-day
remembered cookie. The browser may cache the login according to its own policy.
The owner's generated viewer login is in the private local file
`~/.config/hermes/viewer.json`. Keep it out of messages and Git.

## State and secrets

| Path | Owner and purpose |
| --- | --- |
| `/var/lib/hermes/.hermes` | Gateway databases, memory, skills, schedules and private `.env` |
| `/var/lib/hermes/workspace` | Approved agent files and task notes |
| `/var/lib/camofox` | Browser account, private profiles and engine cache |
| `/var/lib/camofox-downloads` | Temporary downloads, browser-owned and agent-readable |
| `/var/lib/hermes-browser-control` | Private tab/lifecycle record, without a duplicate cookie snapshot |
| `/var/lib/hermes-backups` | Root-only consistent manual backups |
| `/run/secrets/hermes_*_env` | Separate SOPS gateway/browser/controller/proxy environments |
| `secrets/hermes.yaml` | Encrypted source, existing age recipients |

State lives on the internal root SSD. No recurring Hermes job writes to the
HDD backup datasets. Cookies and localStorage persist through graceful stop;
IndexedDB persistence is deliberately disabled until a site needs it and it is
tested. Active DOM/challenges are not durable state.

The Go key is reused from the configured OpenCode Go account. The bot key and
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
private `.env`. Personal homes stay masked; a read-only `/run/user` binding
preserves the dedicated user's bus for native restart-safe scheduled workers.

## Lifecycle and handoff

The Rust controller is the single cleanup authority. Passive page/status/health
requests do not start or renew the browser. Explicit authenticated POST or a
real native Camofox request coalesces startup and waits for `/vnc/status` readiness.
Warm tool requests preserve the tracked tab and reuse validated tab/VNC
readiness while checking the live backend. Only tab identity is persisted;
monotonic in-memory activity avoids wall-clock jumps and per-tool state writes.
A controller restart starts a fresh five-minute idle grace period. A native
missing-tab response invalidates discovery without replaying the failed action.
A lifecycle mutex prevents duplicate
launches and idle shutdown during a real HTTP operation; it is not a human/agent
ownership protocol. No task claims, leases, special handoff tool or resume RPCs.

Cleanup runs after five minutes without real tools or connected viewers.
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

The pinned Camofox server has no lazy-start switch and treats a zero tab timeout
as its default. Small fail-on-drift substitutions disable startup prewarming
and the registered-tab reaper, honor a zero timeout, refuse invisible headless
fallback and use Nix's noVNC path. The orphan-page reaper stays enabled. A small
download-path/mode substitution exposes only temporary downloaded files to the
agent's existing group; it leaves upstream download deletion unchanged.
The existing storage export route requires its persistence listener and
propagates failed writes, so idle cleanup can fail closed without a second
checkpoint implementation or cookie snapshot.
Unpinned automatic add-on downloads are disabled; Camoufox's native
fingerprint behavior remains, but the optional default ad-blocker is absent.

Hermes itself has no source patches or custom plugins. Custom secret
provisioning, browser lifecycle/viewer and owner-operated backup commands are
compiled Rust in `pkgs/host-tools`. Custom operational programs remain Rust;
permanent services and timers belong in Nix and compile during the Nix build.
The default Hermes scheduler is used for ordinary reminders, with no seeded
monitor jobs or custom monitor executable.

`pkgs/host-tools/src/browser_viewer.html` is the small authenticated `/browser/`
page embedded in the Rust controller. It provides Open browser, Disconnect,
passive status and an upstream noVNC canvas. It implements no VNC protocol or
custom login system. An explicit POST keeps page visits from waking Firefox.

Hermes calls native Nix support best-effort. These revisions build natively;
there is no mutable FHS/container installation, runtime compiler or package
download. See [OPERATIONS.md](OPERATIONS.md) and [ACCEPTANCE.md](ACCEPTANCE.md)
for commands, pending production checks and the deployment gate.
