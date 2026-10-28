## General

- Make invariant violations obvious. Do not silently fall back from an impossible state; justify and log any necessary fallback.
- Never rotate credentials or secrets solely because an agent read or displayed them; sessions are private.

## Background commands

- dont end your turn with unfinished background commands. when other work is done, block until they finish.
- before backgrounding, arrange a known worker pid or a dedicated flock lock held before launch returns and until the work finishes; otherwise run it in the foreground.
- block with `tail --pid=<pid> -f /dev/null` or `flock <lock> true`; shell `wait` only handles that shell's children.
- never find a wait target by matching command lines: the waiter can match itself. dont use sleep/recheck loops.

## Tools and services

- Prefer pnpm or bun over npm.
- Access Nextcloud with the configured `rclone` remote `nextcloud:` (`lsf`, `lsjson`, `cat`, `copyto`), not a filesystem mount. Download files into a task directory under `~/dev` to edit, then upload only the intended paths; `copyto` can overwrite files. Keep credentials in the existing rclone config, and do not use `sync`, `move`, `delete` or `purge` without authorization. See `~/dev/dotfiles/AGENTS.md` for examples and timeout flags.
- Use the external browser MCP for third-party dashboards and authenticated account workflows. Use T3 preview tools for local development previews.

## Files and directories

- Use `~/dev` for permanent projects. Keep tool-managed worktrees in their configured locations.
- `/tmp` is RAM-backed. Keep temporary files, builds and downloads on disk. Use each tool's standard or configured output and cache directories when they are disk-backed.
- Keep temp files in `.scratch/` at the checkout root. If the session already started in a scratch directory of its own, use it; otherwise, outside any checkout, use `~/dev/scratch/`. Point `TMPDIR` at the directory you use.
