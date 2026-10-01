---
name: password-manager
description: "Use when the user asks for a stored password, username, TOTP/2FA code, or wants the agent to log into a website on their behalf. Wraps the local Proton Pass CLI (pass-cli)."
---

# Password Manager (Proton Pass via pass-cli)

Fetch credentials from the user's local Proton Pass vaults. Binary: `pass-cli`
(Nix: `proton-pass-cli`; Arch: `proton-pass-cli-bin`).

## Session

- `pass-cli info` shows the current session. If logged out, run `pass-cli login`
  (interactive web login) and hand control to the user.

## Finding items

- `pass-cli vault list` — list vaults.
- `pass-cli item list --vault-name <VAULT> --output json` — list items (id,
  title, item_type). Titles are how items are usually addressed.
- `pass-cli item view --vault-name <VAULT> --item-title <TITLE> --output json`
  — full item. A Login item carries `email`, `username`, `password`, TOTP URI
  and URLs under `item.content.content.Login`.
- Prefer `--field password` (or `--field totp`) when only one field is needed.

## TOTP codes

- `pass-cli item totp --vault-name <VAULT> --item-title <TITLE> --output json`
  — parse the `totp` field. Codes rotate every 30s: generate at the moment of
  use, never store or reuse an old one.

## Logging into a website on the user's behalf

1. Locate the Login item for the site (match title/URL).
2. Pull username + password (via `--field` or JSON parsing, never `grep` on
   human output).
3. Fill the site's login form (see browser-automation skill), TOTP last and
   fresh.

## Rules

- Never print secrets (passwords, TOTP codes, recovery words) into chat,
  logs, or files. Pipe them through variables; `shred -u` any temp file.
- Parse `--output json` with `python3`/`jq`; do not scrape human-readable output.
- `pass-cli run -- <cmd>` can inject secrets as env vars into a subprocess
  without them touching disk — prefer it for scripted logins.
