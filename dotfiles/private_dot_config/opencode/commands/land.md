---
description: Commit and land this session's branch on main, or queue it when another session blocks landing. Use ONLY when the user explicitly requests landing.
model: openai/gpt-6-luna#max
---

If there are still uncommitted changes, read and follow
`~/.config/opencode/commands/commit.md`. Create a task branch if HEAD is detached.
Rebase onto local main, resolving conflicts while preserving both branches' intended changes,
then merge into its checkout with `git merge --ff-only --no-autostash`.
If main gains new commits, repeat the rebase and merge until landing succeeds.

Keep checks proportional and reuse valid results already obtained.

Main may contain another session's uncommitted work. Merge if Git permits
it without overwriting those changes; never stash, commit, or discard them.
Only if main's uncommitted changes block landing, leave main alone and rename the task branch
`merge-ready-<branchname>`, keeping it for later.

After landing or queuing, remove the clean temporary worktree with
`git worktree remove`, from main's directory as the final tool call.
Delete the task branch only if successfully merged. Do not push.

Briefly report whether landed or queued, the branch/commits, checks,
and cleanup.
