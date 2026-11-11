# Claude and OpenAI API credit routing

## Private audit history

Each machine keeps a durable, metadata-only trail beside its router state:
`~/.local/state/credit-router/<vendor>-credit-router/audit-spool/` and
`audit.sqlite3`. Directories are private (0700); spools/database are 0600.
The CLI imports append-only spools transactionally with checkpoint/event-ID
deduplication. There is no 32-event retention limit on the audit: the smaller
state history is still only a status convenience. No automatic deletion occurs.

```sh
openai-credit-router history --limit 100
claude-credit-router history --summary --grant-cycle
openai-credit-router history --summary --since 2026-11-10T00:00:00Z
openai-credit-router history --summary --session ses_EXAMPLE
```

Records distinguish **actual router attempts** from **passive OpenCode usage**:

- Actual API/native route, fallback reason, UTC time/machine, HTTP outcome and
  duration; safe upstream request/response IDs; model and reported usage when
  exposed. Accepted streams are forwarded unchanged, never replayed for logging.
- Public OpenCode step and compaction accounting, including native requests that
  bypass the router. Title usage is unavailable through the installed public
  event API; a prepared-request record is not proof of transport or billing.
- Full future switch/manual-override history. The installation coverage marker
  explicitly warns that earlier history is incomplete; only the old retained
  state events can be imported. Crashes can leave an unmatched request-start.
- Exact OpenCode session/parent IDs and T3 thread bindings. Reports read only
  native-binding identifiers from `~/.t3/userdata/statev2.sqlite` (override with
  `--t3-db`). Claude CLI session UUIDs are joined only when public native metadata
  exposes an unambiguous binding. Otherwise requests remain **uncorrelated**;
  model/time/title similarity is never used to guess a thread.

Public models.dev API-price snapshots are timestamped and used only for API
**token-cost estimates**, never subscription bills. Missing final usage, unknown
prices/cache TTLs/nonstandard service tiers are explicitly missing, not zero.
`api_cost_missing` counts excluded requests; any displayed estimate is a partial
sum if that count is nonzero. Native OpenCode-reported costs remain a separate
field and may be zero due to subscription metadata. Do not add router and
OpenCode token totals: they are different observations of overlapping work.
Vendor invoice/credit totals, tool charges and exact shared-organization
promo-versus-paid attribution cannot be reconstructed from these estimates.

Prompts, response text, tool inputs/results, raw error text, cookies and
authentication headers are **never written**. Subscription responses are not
parsed. API response parsing is bounded; absent/oversized usage events remain
unknown. Audit writes are private fsynced spools; SQLite ingestion/reporting is
off the inference/status critical path. An audit failure is conspicuous in the
service/plugin log but must not change routing, authentication or retry policy.
No new credentials or browser logins are required.

The OpenCode extension re-registers its provider transform after plugin activation:
OpenCode 2.2 applies explicit provider configuration after ordinary plugin setup.
This keeps the original configured subscription endpoint as the transform's input,
so configuration cannot silently override promotional routing. No session, auth or
subscription-retry hooks are installed.

Uses a personal, promotional-only Anthropic API organization before the existing
Claude Agent subscription connection. A separate OpenAI instance routes direct
Responses API requests through the dedicated project before native Codex usage.
Neither instance controls subscription limits, retries or gifted credits.

## OpenAI shared-organization safeguard

The first grant is **$199**, expiring December 9, 2026; later grants follow the
ChatGPT subscription cycle. The dedicated `voxride-opencode` project has a
**$100 enforced dashboard limit** and a separate **$100 local admission limit**.
Neither limit allocates a funding source: organization credits remain shared.

Before every promotional Responses or compaction request, the router checks a
60-second cache of the project's calendar-month USD costs from OpenAI's Costs API.
All OpenCode instances on one machine share its router's cache, including status
polls; each machine refreshes independently, but reports include both machines and
other users of that project. Expired snapshots require a successful refresh before
API admission; they are never used while refreshing. It uses the existing private
CodexBar `openai` reporting credential (Usage Read); CodexBar need not be running.
It never sends that credential to inference or Sleev. `check-budget` bypasses the
cache for an explicit refresh. Daily UTC buckets include the entire current
day because an intraday end time can omit today's spend.

At or above the local limit, the router records `local_spend_limit`, notifies the
user, and forwards the unchanged request using native subscription authentication.
Cached reports are bound to their organization and project; older unbound reports
require a fresh check. It stores a monthly spending high-water mark, so restarts, stale lower reports,
and manual rechecks cannot forget known exhaustion. Missing credentials, HTTP
errors, malformed data, incomplete coverage or unexpected projects/currencies
record `spend_check_unavailable` and use subscription with a one-minute retry.
Subscription requests do not read cost reports or change native retry behavior.

The API window remains the renewal day (9th UTC) through month-end. Days 1–8 use
native Codex: a calendar reset must not allow a second allowance within the same
grant cycle. At renewal, a fresh report is required before any API inference.
Manual subscription mode overrides renewal until explicitly changed.

**This is a reported-spend cutoff, not an exact token ledger or a $0-charge
guarantee.** The 60-second cache, reporting lag and accepted concurrent requests can exceed
$100. There is no shared in-flight reservation across machines. Passing mocked
cutoff tests does not establish a bound on OpenAI's reporting delay, and does not
justify raising this to $190. Token-based audit estimates have incomplete usage
and pricing and are not safe substitutes. Keep the $100 margin against a full $200 grant; stronger guarantees
need coordinated conservative reservations or a separate org without paid funds.
The router does not modify organization-wide billing or company project settings.

The router uses `OPENAI_CREDIT_ROUTER_API_KEY` from the same SOPS environment file,
never `OPENAI_API_KEY`. Its free Models request verifies project access and loads
the API model inventory. API-only HTTP routing preserves model IDs, reasoning,
encrypted history and tool bytes; Codex-only models remain native and record a
reason instead of being silently mapped. Responses and Responses compaction use
the direct API (not the Codex product's API-key billing route).

In addition to the local admission check, exact pre-output billing codes (`project_spend_limit_exceeded`,
`organization_spend_limit_exceeded`, `organization_usage_limit_exceeded`,
`credit_balance_exhausted`) switch to native authentication. Temporary 429s,
ambiguous `insufficient_quota`, authentication failures, partial streams, native
subscription errors and gifted-credit behavior are not rerouted.

Use `openai-credit-router status|subscription|auto|recheck`; `recheck` cannot bypass
the calendar guard or a known exhausted budget. `openai-credit-router check-budget`
refreshes costs and applies the local limit without making any inference request
or clearing manual subscription mode. Status exposes the latest reported USD,
check timestamp, local limit and reason history. Inspect `journalctl --user -u openai-credit-router` or the
loopback `http://127.0.0.1:17323/_credit/status` for route and reason history.
The native provider endpoint and transport are restored within five seconds of a
route change; the already-prepared rejected request is forwarded once natively.
Renewal uses the next real model request, never a test inference.

## Claude setup

The workstation Home Manager module installs the compiled router, its user service
and the OpenCode extension. The service and extension stay inactive until
`~/.config/claude-credit-router/config.json` exists. Do not add that file until the
personal organization's billing and key ownership have been verified.

Example configuration (replace the organization, paths and verification date):

```json
{
  "listen": "127.0.0.1:17322",
  "upstream": "http://127.0.0.1:17321/sleev/opencode/anthropic",
  "organization_id": "VERIFIED-PERSONAL-API-ORGANIZATION-ID",
  "renewal_anchor": "2026-12-03T00:00:00Z",
  "billing": {
    "verified_at": "2026-11-10T00:00:00Z",
    "no_payment_method": true,
    "purchased_balance_usd": 0,
    "auto_reload": false,
    "sales_invoicing": false
  },
  "notify_send": "/home/USER/.nix-profile/bin/notify-send"
}
```

Store the key in SOPS-managed `~/.config/environment.d/55-secrets.conf` as
`CLAUDE_CREDIT_ROUTER_API_KEY`. The service reads that environment file directly;
new login sessions also inherit it. Never use `ANTHROPIC_API_KEY`: that could change
native Claude subscription authentication. Alternatively, set `api_key_file` to a
private regular file (0600/0400); a SOPS symlink to such a file is accepted.
Never supply subscription OAuth credentials. The service validates the API key's
organization using the free Models metadata endpoint before listening. Missing or
unexpected organization headers block startup rather than guessing.

After activating Home Manager, refresh the SOPS-managed environment file with
`systemctl --user restart sops-nix`. Arch activation deliberately does not restart
running user services. Apply the router's chezmoi configuration, then start with
`systemctl --user start claude-credit-router`. The extension notices it
within five seconds without restarting OpenCode. It wraps only the configured
Claude Agent Sleev endpoint, not per-session model/reasoning settings or auth.

## Claude routing and renewal

Only POST `/v1/messages` uses promotional authentication. This covers Claude Agent's
model calls regardless of whether OpenCode initiated them for a prompt, subagent,
title, generation or compaction. All other endpoints retain native authentication.
In particular, free token counting is not evidence that paid model credits renewed.

The router substitutes an API key only on the upstream promotional request and
removes the OAuth-authentication beta flag. It preserves model IDs, reasoning,
signed thinking, tool definitions and conversation bytes. It does not change
product identity or rewrite requests to evade product restrictions. Agent SDK API
usage is eligible for the benefit; Claude Code product usage is not. If Anthropic
rejects the request as ineligible, that error is not treated as credit exhaustion.

Only the documented full credit-depletion JSON message, with HTTP 400/402 and the
matching error type, permits one subscription attempt of the rejected request.
Temporary limits, spend caps, auth failures, network failures and unknown billing
errors are returned unchanged. HTTP 200 streams are never altered or replayed,
even when they contain an SSE error after output/tool calls have started.

Anthropic can also end a promotional response mid-stream with that same depletion
message, when the balance reaches zero while the response is being generated.
The router forwards the stream as it is and keeps the route: only a rejection
before any output is safe to replay. It lists the cut under `interrupted` in
`/_credit/status` for ten minutes, with the Claude Code session it belonged to.
OpenCode never retries Claude Agent failures, so the extension resends that one
turn, once per cut and session. The resent request is rejected before output and
takes the path above. A failure without a matching entry is left alone, which
includes every subscription failure.

After exhaustion, subscription requests are forwarded without response inspection
or retries. Within five seconds the extension removes its endpoint override, so
subsequent native Claude Agent processes bypass the router entirely. An already
running process can continue through the transparent subscription path. Native
Claude/OpenCode subscription retries and errors remain theirs to handle.

Routing state is atomically persisted under
`~/.local/state/credit-router/claude-credit-router/state.json` (or the equivalent
`openai-credit-router` subdirectory), with file locking, current mode,
renewal timestamp and the last 32 reasons. Invalid state is an explicit error, not
an implicit reset to the API route. Monthly renewal uses the verified subscription
billing anchor, not the first of the calendar month. The next real request probes
the renewed grant; there are no additional inference probes. A delayed grant is
retried at most every six hours without announcing false route changes.

Desktop notifications require `notify_send` to point to an installed `notify-send`.
The service journal also records route changes without keys, tokens or request bodies.

```sh
claude-credit-router status        # route, override, next probe and reasons
claude-credit-router subscription  # keep native subscription usage
claude-credit-router auto          # resume automatic routing
claude-credit-router recheck       # probe on the next real model request
journalctl --user -u claude-credit-router
```

## Claude billing boundary

Use a separate personal organization with **no purchased credits, payment method,
auto-reload or sales invoicing**. Do not add a payment method or buy credits later
without first disabling this feature and re-evaluating the safeguards. Exhaustion
errors describe the *total* credit balance, not promotional credits separately;
mixed funding is unsafe. A $0 total-usage cap also blocks promotional usage.

The configuration records a billing verification; it is not an authoritative live
balance feed and cannot prevent account owners changing billing afterward. The
router does not cap token spending, estimate balances or replace vendor billing
enforcement. No guaranteed instantaneous cash-only $0 spending control is assumed.
Only the vendor's promotional-only/no-paid-funding setup prevents paid continuation.
The loopback transport assumes a trusted local workstation, like the existing
Sleev gateway; do not publish it through a reverse proxy or treat it as a
multi-user authentication service. Browser-origin inference requests are refused.

Tests use local mock servers and fabricated credentials. They cover exhaustion,
transient limits, native subscription failures, renewal/delayed grants, restarts,
concurrent rejection, exact history preservation, streaming no-replay, organization
validation and unsafe billing configurations. They do not prove live Agent SDK
request compatibility or the vendor's billing enforcement. Enable only after the
account/key checks; no real inference request is required for installation tests.
