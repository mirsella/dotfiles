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

## Hermes Camofox fill

Saved credentials may be read in private tools for the owner's requested tasks.
For browser fills, prefer this bridge: it reads the secret command's stdout in
memory and types into the native Camofox input without returning the value.
Use the page's exact HTTPS origin and an input ref from the latest snapshot:

```sh
host-tools browser-secret --origin https://login.example.com --ref e12 -- \
  pass-cli item view --vault-name Personal --item-title TITLE --field password
```

For TOTP, add `--json-pointer /totp` and use
`pass-cli item totp --vault-name Personal --item-title TITLE --output json`.
If several tabs match the origin, the bridge reports their IDs; select one with
`--tab ID`. It checks the origin again under the native tab lock before filling.
The browser also refuses the fill when the page, or the input's own document, is
no longer on that origin. Later text responses show `[redacted]` instead of the
typed value, and that marker means the fill landed, so do not retype it.
Screenshots and downloads still show the value, so never screenshot a filled
field to check it: `[redacted]` is the proof the fill landed.
It does not submit the form. Continue with normal browser tools afterward.

Hermes's built-in browser vault is CDP-only and does not support Camofox or Proton
Pass. Use this bridge instead of asking the owner to duplicate saved credentials.
Use the Mail skill for emailed verification codes; ask for human help only when
CAPTCHA, device approval or a hardware key requires it.
