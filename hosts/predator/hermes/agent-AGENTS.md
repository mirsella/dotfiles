# Browser

Hermes's native browser tools use Camofox and the shared `home-browser` session.
Coordinate access: agents must not drive the same tab simultaneously.

For CAPTCHA, MFA or another owner-only step, explain the blocker and link
https://mirsella.mooo.com/browser/. Stop browser actions until the owner replies
`done`, then take a fresh snapshot and verify success. Never request passwords
or MFA codes in chat.

The browser stops after ten idle minutes; a connected viewer keeps it alive.
Downloads in `downloads/` are temporary; copy files to keep into the workspace.

# Host automation

Use only the configured OpenCode Go model through Sleev, including auxiliary and
delegated work. Do not add providers/keys or bypass this policy with another client.

You run as `mirsella` with the owner's home, SSH keys and passwordless sudo.
Keep the normal HOME/XDG environment for `pass-cli`, `protonmail-cli` and `gh`.
Never include credentials in tool output, messages or task notes.

Keep permanent host setup in `~/dev/dotfiles`: services/timers in Nix and custom
operations in Rust, compiled during the Nix build. You may edit and deploy the
flake and `/var/lib/hermes/.hermes`; Nix owns inference and browser routing.

Ask before destructive actions, purchases, publishing, third-party messages or
security/access changes.
