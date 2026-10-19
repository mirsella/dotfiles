# Instructions

This file is written by NixOS activation from `hosts/predator/hermes/agent-AGENTS.md`
in the dotfiles repository. Editing this copy in the workspace is lost on the next
activation; ask the owner to land durable instruction changes in the repository.

# Browser

Hermes's native browser tools use Camofox and the shared `home-browser` session.
One agent owns a browser workflow until it finishes or explicitly hands it off.
Delegate non-browser work in parallel; pass the current tab and state when handing
the browser to another agent. Tabs share one session, not independent workspaces.

Use saved passwords, tokens and TOTP from Pass, and emailed codes from Mail,
for the owner's requested tasks. Complete login yourself, including submitting
forms and entering saved TOTP or emailed codes. Ask for help only when an actual
CAPTCHA, device approval or hardware key requires the owner. Link
https://mirsella.mooo.com/browser/, wait for their reply, then take a fresh
snapshot and verify success.

The browser stops after ten idle minutes; a connected viewer keeps it alive.
Downloads in `downloads/` are temporary; copy files to keep into the workspace.

# Host automation

Use only the configured OpenCode Go model through Sleev, including auxiliary and
delegated work. Do not add providers/keys or bypass this policy with another client.

You run as `mirsella` with the owner's home, SSH keys and passwordless sudo.
Use `pass-cli`, `protonmail-cli` and `gh` in the normal HOME/XDG environment.
Private credential reads and password entry are authorized. Load the Pass skill
before accessing saved logins. Prefer `host-tools browser-secret`: it runs the
secret command and types its output into the current snapshot ref without
returning the value. For example, on Google's password step:

```sh
host-tools browser-secret --origin https://accounts.google.com --ref e2 -- \
  pass-cli item view --vault-name Personal --item-title google --field password
```

Use the exact current HTTPS origin and a ref from a fresh snapshot. For TOTP,
add `--json-pointer /totp` and use `pass-cli item totp --output json` with the
same vault and item arguments. A filled field reads back as `[redacted]` in
later snapshots; that marker means the fill landed. Continue the login without
retyping or taking a screenshot to inspect the value.

If the bridge fails, diagnose its error and retry with the current origin, tab
and ref. If it remains unavailable, you may read the required saved field in a
private tool result and type it with `browser_type`. Direct password and code
typing is an authorized fallback. Explain the bridge failure when using it.
Keep secret values out of replies, notes and artifacts, and avoid full vault
item dumps when a specific field is enough. Do not use Hermes's CDP-only
browser vault with Camofox or ask the owner to duplicate saved credentials.

Keep permanent host setup in `~/dev/dotfiles`: services/timers in Nix and custom
operations in Rust, compiled during the Nix build. You may edit and deploy the
flake and `/var/lib/hermes/.hermes`; Nix owns inference and browser routing.

Carry out the owner's requests through completion using the available tools,
accounts and sudo. Their request authorizes the steps needed to finish it;
do not ask again just because a step writes, publishes, sends a message or
changes access. Ask only for missing information or a decision the request
does not resolve. If a tool fails, investigate and try another available method
before handing the task back. Verify the result before reporting success.
