# Local widget patches

`api-credit-rows.patch` adds opt-in read-only billing rows inside the existing
Claude and Codex cards. It does not enable new provider cards, alter subscription
usage or gifted Codex credits, make inference requests, change billing settings,
or enable the credit router.

- **Claude:** the active subscription API-credit tranche reported by CodexBar's
  Claude Platform source. Shows dollars used/total, remaining and expiry. Purchased
  credit and local Claude token estimates are never treated as promotional credit.
- **Codex:** calendar-month USD costs for the configured OpenAI API project against
  its configured display budget ($150 here). This is **not** the organization's
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
can expire and require signing in again. The widget never extracts browser cookies.

### Encrypted shared configuration

The complete shared configuration is encrypted as `config_json` in
`secrets/codexbar.yaml`, using the repository's existing age recipients. Home
Manager/SOPS installs its private restore copy at
`~/.config/codexbar/shared-config.json` (0600) on both workstations. No plaintext
configuration enters Git or the Nix store.

The live `~/.config/codexbar/config.json` remains a regular writable 0600 file:
CodexBar atomically replaces it when saving settings, so it must not be a SOPS
symlink. Existing live settings are never overwritten automatically. To restore
the snapshot, explicitly replace the live file while holding CodexBar's lock:

```sh
flock ~/.config/codexbar/config.json.lock sh -c '
  set -e
  cd ~/.config/codexbar
  install -m 600 shared-config.json config.json.restore
  mv config.json.restore config.json
'
```

After changing settings or refreshing an expired cookie, deliberately update the
encrypted snapshot before sharing it. With SOPS decryption configured, capture
without displaying or writing an extra plaintext copy:

```sh
jq --null-input --rawfile config ~/.config/codexbar/config.json '$config' |
  sops set --value-stdin secrets/codexbar.yaml '["config_json"]'
```

Commit only the encrypted YAML, then rebuild and restart `sops-nix.service` on
the other workstation to refresh its restore copy. Restoring it remains explicit.

## Data and validation

Missing, expired, stale, malformed or incomplete billing data shows **Unavailable**,
never an invented zero. In particular, CLI 0.84 returns 30 daily buckets even when
`OPENAI_HISTORY_DAYS` is set: on the 31st of a month, the widget refuses to claim a
complete monthly total if the first day is absent. Daily costs can also be delayed
or revised by OpenAI. This display is not a financial enforcement mechanism.

The package runs all upstream JavaScript tests plus the billing-row regressions
after applying the three patches. Reload the widget or log out/in after updating;
the build does not restart Plasma automatically.
