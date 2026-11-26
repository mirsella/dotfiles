## General

- Make invariant violations obvious. Do not silently fall back from an impossible state; justify and log any necessary fallback.
- Never rotate credentials or secrets solely because an agent read or displayed them; sessions are private.
- When a change is visible (UI, layout, animation, a report), show it inline at the end of the turn without being asked: the result for something new, before and after side by side for a change to something existing. Use screenshots for static states and a recording when motion or interaction is what changed. Label each capture and look at it yourself first.
- When comparing source checkouts, give each checkout its own Cargo target/build directories, including baseline worktrees; reuse the shared Kache cache instead of sharing Cargo outputs.

## Background commands

- dont end your turn with unfinished background commands. when other work is done, block until they finish.
- before backgrounding, arrange a known worker pid or a dedicated flock lock held before launch returns and until the work finishes; otherwise run it in the foreground.
- block with `tail --pid=<pid> -f /dev/null` or `flock <lock> true`; shell `wait` only handles that shell's children.
- never find a wait target by matching command lines: the waiter can match itself. dont use sleep/recheck loops.

## Tools and services

- Prefer pnpm or bun over npm.
- Access Nextcloud with the configured `rclone` remote `nextcloud:` (`lsf`, `lsjson`, `cat`, `copyto`), not a filesystem mount. Download files into a task directory under `~/dev` to edit, then upload only the intended paths; `copyto` can overwrite files. Keep credentials in the existing rclone config, and do not use `sync`, `move`, `delete` or `purge` without authorization. See `~/dev/dotfiles/AGENTS.md` for examples and timeout flags.
- Before asking for a VoxRide token, check the `secretspec.toml` scopes and `secrets/` payloads of mightyminions, voxride-dashboard and voxride-website. Store a new long-lived token in the owning repository's SOPS payload so later sessions reuse it.
- Use the external browser MCP for third-party dashboards and authenticated account workflows. Use T3 preview tools for local development previews.
- When the browser MCP fails, do not fall back to computer use. Fix the browser MCP at its source so it works again for this and every other session, then continue the task with it.
- Never call the T3 desktop `browser.*` tools; they are never attached to service sessions and only return a disconnect error.

## Files and directories

- Use `~/dev` for permanent projects.
- `/tmp` is RAM-backed: put builds, large downloads and generated files in the scratch directory.
- Scratch directory: `.scratch/` at the root of the checkout you work in, including clones of other repositories and extra Cargo targets; in a projectless T3 thread, its working directory; otherwise `~/dev/scratch/<task>/`.
- Leave temporary artifacts, including screenshots and recordings you show, in scratch: sweep removes them with their workspace. If the user asks to keep one, move it where they say, or ask where. Never run `sweep keep` unless the user asks for it by name.
- Do not set `TMPDIR` to a scratch directory, and never remove a temporary root while processes still use it.
