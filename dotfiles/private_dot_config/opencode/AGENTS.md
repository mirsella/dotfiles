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
- Never call the T3 desktop `browser.*` tools; they are never attached to service sessions and only return a disconnect error.

## Files and directories

- Use `~/dev` for permanent projects. Keep tool-managed worktrees in their configured locations.
- `/tmp` is RAM-backed. Keep builds, large downloads and large generated files in the scratch directory. Small tool-managed temporary files may use their normal temporary directory.
- Where the session runs decides the scratch directory, not what the files are:
  - In a checkout or worktree: `.scratch/` at its root, for everything the task produces. That includes clones of other repositories, downloads, captures, backups and extra Cargo target directories.
  - In a T3 thread without a project: its working directory, `~/.t3/scratch/<thread>`.
  - Anywhere else, including a host reached over ssh: `~/dev/scratch/<task>/`.
- Scratch is disposable, and `.scratch/` is deleted with its worktree. Before settling, copy out only the files the user asked to keep, to `~/dev/scratch/<project>-<thread>/`, and report the path.
- Do not set `TMPDIR` to a checkout/worktree scratch directory. Preserve tool-managed temporary environments, and never remove a temporary root while processes still use it.

## Worktrees

When your session, or the main thread you work for, runs in a T3 worktree (`~/.t3/worktrees/...`):

- Put extra worktrees inside it: `git worktree add -b <branch> <t3 worktree>/.scratch/worktrees/<name>`. Settling the thread then deletes them.
- Do not use `opencode.worktree_create` or `session_move`: they put worktrees outside the thread.
- Settling deletes nested worktrees without checking for changes. Before ending a turn, commit work you want to keep to the worktree's branch, never a detached HEAD.
