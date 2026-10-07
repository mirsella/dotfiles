---
name: mail
description: "Use Proton Mail to read or search messages and retrieve emailed verification codes. Use the pass skill for saved passwords and TOTP codes."
---

# Proton Mail

Use `protonmail-cli` (profile `default`; override with `--profile <NAME>`). Most commands accept `--json`.

## CLI reference

- Account: `protonmail-cli whoami`.
- Recent unread: `protonmail-cli messages list --folder inbox --unread --limit 10 --json`.
- Read: `protonmail-cli messages read <ID>`.
- Search: `protonmail-cli messages search <QUERY> --json`. Prefer this server-side search over bare `search`, which uses an offline cache requiring `sync` and `index`.

If login is needed, get `PROTON_USER`, `PROTON_PASSWORD`, and `PROTON_TOTP` through the pass skill. Let the user handle any CAPTCHA, then continue.

For verification codes, read the newest matching message and return only the code.
