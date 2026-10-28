- Make invariant violations obvious. Do not silently fall back from an impossible state; justify and log any necessary fallback.
- Never rotate credentials or secrets solely because an agent read or displayed them; sessions are private.
- If you have nothing else to do and a background process running, block on it instead of returning. dont stop if you have background stuff running.
- When you need to wait for something wait on it directly instead of sleeping. for example on the pid instead of sleep and rechecking in between.

- Prefer pnpm or bun over npm.
- Access Nextcloud with the configured `rclone` remote `nextcloud:` (`lsf`, `lsjson`, `cat`, `copyto`), not a filesystem mount. Download files into a task directory under `~/dev` to edit, then upload only the intended paths; `copyto` can overwrite files. Keep credentials in the existing rclone config, and do not use `sync`, `move`, `delete` or `purge` without authorization. See `~/dev/dotfiles/AGENTS.md` for examples and timeout flags.
- Use the external browser MCP for third-party dashboards and authenticated account workflows. Use T3 preview tools for local development previews.

- Use `~/dev` for permanent projects. Keep tool-managed worktrees in their configured locations.
- `/tmp` is RAM-backed. Keep temporary files, builds and downloads on disk. Use each tool's standard or configured output and cache directories when they are disk-backed.
- Keep temp files in `.scratch/` at the checkout root. If the session already started in a scratch directory of its own, use it; otherwise, outside any checkout, use `~/dev/scratch/`. Point `TMPDIR` at the directory you use.
