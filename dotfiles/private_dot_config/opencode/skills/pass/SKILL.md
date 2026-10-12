---
name: pass
description: Use Proton Pass for saved logins and TOTP codes; use mail for emailed codes.
---

Use the matching saved login automatically. Prefer extension autofill, then `pass-cli`.
Run the CLI in the normal user environment: sanitized HOME/XDG settings can lose
its encryption-key context and log it out. Check `pass-cli info`; if needed, start
`pass-cli login` for the user to finish.

- Discover: `vault list --output json`, then `item list --vault-name VAULT --output json`.
- Read: `item view --vault-name VAULT --item-title TITLE --field password`; full JSON uses `item.content.content.Login`.
- TOTP: `item totp --vault-name VAULT --item-title TITLE --output json`, field `totp`.

Use variables or `pass-cli run -- COMMAND` for credential handoff.
