# Local widget patches

`api-credit-rows.patch` adds opt-in read-only billing rows inside the existing
Claude and Codex cards. It does not enable new provider cards, alter subscription
usage or gifted Codex credits, make inference requests, change billing settings,
or enable the credit router.

- **Claude:** the active subscription API-credit tranche reported by CodexBar's
  Claude Platform source. Shows dollars used/total, remaining and expiry. Purchased
  credit and local Claude token estimates are never treated as promotional credit.
- **Codex:** calendar-month USD costs for the configured OpenAI API project against
  its configured display budget ($190 here). This is **not** the organization's
  promotional balance. Project caps reset independently from subscription grants;
  billing data/enforcement can lag.

Enable **API billing rows inside Claude and Codex cards** in General settings;
configure the project ID and budget there. The repository's Plasma template
enables this for `voxride-opencode`. The amount is a display setting, not a limit
setter. Data is fetched using existing CLI probe scheduling and deduplication.
API-only sources do not generate subscription notifications or panel meters.

## Credentials

Credentials stay in CodexBar's private configuration, not the widget, patch,
Plasma settings or Nix store. Subscription credentials remain unchanged.

```sh
# Linux: pipe the platform.claude.com Cookie header securely, never via argv.
codexbar config login --provider claudeplatform --stdin --json
codexbar config disable --provider claudeplatform

# A separate OpenAI Admin key with only Usage API Scope: Read.
codexbar config set-api-key --provider openai --stdin --no-enable
```

The OpenAI reporting key can read organization usage; it is not restricted to a
project at the permission level. The widget explicitly sets `OPENAI_PROJECT_ID`
for its OpenAI probe and rejects unscoped/mismatched reports. The project's
existing inference key is insufficient for reporting. Claude Console sessions
can expire and require signing in again. No automatic cookie extraction occurs
in the widget, and credentials are not automatically copied to another machine.

Missing, expired, stale, malformed or incomplete billing data shows **Unavailable**,
never an invented zero. In particular, CLI 0.84 returns 30 daily buckets even when
`OPENAI_HISTORY_DAYS` is set: on the 31st of a month, the widget refuses to claim a
complete monthly total if the first day is absent. Daily costs can also be delayed
or revised by OpenAI. This display is not a financial enforcement mechanism.

The package runs all upstream JavaScript tests plus the billing-row regressions
after applying the three patches. Reload the widget or log out/in after updating;
the build does not restart Plasma automatically.
