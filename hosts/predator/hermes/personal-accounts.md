---
name: personal-accounts
description: Use the owner's Proton Pass, Proton Mail and GitHub accounts.
---

You run as `mirsella` with HOME `/home/mirsella`. Keep the normal HOME and XDG
environment when using account tools. Do not print credentials in tool output,
messages or task notes.

## Proton Pass

Use `pass-cli info`, then `pass-cli vault list --output json` and
`pass-cli item list --vault-name VAULT --output json` to find a matching login.
Read it with `pass-cli item view --vault-name VAULT --item-title TITLE --output json`.
Login fields are under `item.content.content.Login`. Get TOTP with
`pass-cli item totp --vault-name VAULT --item-title TITLE --output json`.
Prefer `pass-cli run -- COMMAND` or in-memory variables for credential handoff.

## Proton Mail

Use `protonmail-cli whoami` to check the session. `protonmail-cli messages list
--unread --limit 25`, `messages read ID`, and `search "query"` read mail. Use
`--profile default` unless the owner requests another profile. Forwarded Voxride
and secondary Gmail messages arrive in this mailbox.

## GitHub

Use `gh auth status`, then native `gh` commands for repositories, pull requests,
issues and checks. Git and SSH use the owner's existing dotfiles configuration.

## Inference

OpenCode Go, OpenCode Zen, OpenAI API and OpenAI Codex use the local Sleev gateway
at `127.0.0.1:17321`. Go/Muse remains the initial default with no automatic model
fallback. Use a separate native Codex login for Hermes rather than copying a
rotating OAuth refresh token from another application.
