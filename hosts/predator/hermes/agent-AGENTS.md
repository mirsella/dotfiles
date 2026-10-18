# Instructions

This file is written by NixOS activation from `hosts/predator/hermes/agent-AGENTS.md`
in the dotfiles repository. Editing this copy in the workspace is lost on the next
activation; ask the owner to land durable instruction changes in the repository.

# Browser

Hermes's native browser tools use Camofox and the shared `home-browser` session.
One agent owns a browser workflow until it finishes or explicitly hands it off.
Delegate non-browser work in parallel; pass the current tab and state when handing
the browser to another agent. Tabs share one session, not independent workspaces.

For CAPTCHA, MFA or another owner-only step, explain the blocker and link
https://mirsella.mooo.com/browser/. Stop browser actions until the owner replies
`done`, then take a fresh snapshot and verify success. Use saved passwords,
tokens and TOTP from Pass, and emailed codes from Mail, for the owner's
requested tasks. Never request passwords or MFA codes in chat.

The browser stops after ten idle minutes; a connected viewer keeps it alive.
Downloads in `downloads/` are temporary; copy files to keep into the workspace.

# Host automation

Use only the configured OpenCode Go model through Sleev, including auxiliary and
delegated work. Do not add providers/keys or bypass this policy with another client.

You run as `mirsella` with the owner's home, SSH keys and passwordless sudo.
Use `pass-cli`, `protonmail-cli` and `gh` in the normal HOME/XDG environment.
Private credential reads are authorized. Load the Pass skill and fill saved logins
with its `host-tools browser-secret` bridge: it runs the secret command, types the
value into the snapshot ref and never returns it. A filled field reads back as
`[redacted]` in later snapshots; that marker means the fill landed, so do not
retype it or screenshot the field to check it. Do not copy logins into Hermes's
CDP-only browser vault. Keep secrets out of tool output, messages, notes and
third-party artifacts.

Keep permanent host setup in `~/dev/dotfiles`: services/timers in Nix and custom
operations in Rust, compiled during the Nix build. You may edit and deploy the
flake and `/var/lib/hermes/.hermes`; Nix owns inference and browser routing.

Ask before destructive actions, purchases, publishing, third-party messages or
security/access changes.
