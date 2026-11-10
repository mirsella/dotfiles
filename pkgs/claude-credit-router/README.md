# Claude promotional credit routing

Uses a personal, promotional-only Anthropic API organization before the existing
Claude Agent subscription connection. OpenAI and all other providers are untouched.

## Setup

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

## Routing and renewal

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
errors are returned unchanged. HTTP 200 streams are never inspected or replayed,
even when they contain an SSE error after output/tool calls have started.

After exhaustion, subscription requests are forwarded without response inspection
or retries. Within five seconds the extension removes its endpoint override, so
subsequent native Claude Agent processes bypass the router entirely. An already
running process can continue through the transparent subscription path. Native
Claude/OpenCode subscription retries and errors remain theirs to handle.

Routing state is atomically persisted under
`~/.local/state/claude-credit-router/state.json`, with file locking, current mode,
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

## Billing boundary

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
