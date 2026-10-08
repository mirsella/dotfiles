# Shared browser

Use Hermes's built-in Camofox tools and the shared `home-browser` session.
Keep browser work in the main agent; delegate other work, not simultaneous
actions in this browser. Do not use CDP, browser_exec or another browser.

For login, CAPTCHA, MFA or another step needing the owner:
1. Save the current URL, blocker and intended next step in the task notes.
2. Reply in this conversation with a short explanation and
   https://mirsella.mooo.com/browser/. Ask the owner to finish the step and reply
   `done`. End the turn and stop browser actions while waiting.
3. Wait for the owner to reply `done` in this conversation before rechecking.
   Take a fresh snapshot and verify the blocker is cleared before acting.
   If the live page/challenge was lost to idle shutdown or restart, say so and
   restart that step. Do not assume a message alone proves the login succeeded.

The viewer has no ownership lock. Stopping browser actions is your responsibility.
Do not poll, take snapshots or make browser requests while waiting. Do not
create a timer or background job to recheck; wait for `done`.

Describe human blockers clearly so Hermes's native goal judge can classify the
goal as blocked. Native `/goal pause` and `/goal resume` remain owner controls;
there is no custom goal/resume hook or hidden budget reset.

The browser may stop after five minutes without tool/viewer activity. An open,
connected viewer keeps it alive. Downloads are temporary and read-only under
`downloads/`; copy a requested file elsewhere in the workspace before cleanup.
Never ask the owner to send passwords or MFA codes in chat.

# Host automation

Use the configured model and native Hermes tools. Keep custom operational
programs in Rust and permanent services/timers in Nix. Deployed Rust programs
must be compiled during the Nix build, not on their first start. Ask the owner
to review/deploy host changes; this account has no sudo or personal SSH keys.
Ask before destructive actions, purchases, publishing, third-party messages or
security/access changes. Do not include credentials in messages or task notes.
