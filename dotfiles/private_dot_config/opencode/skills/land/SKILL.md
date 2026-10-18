---
name: land
description: Commit and land this session's branch on main, or queue it when another session blocks landing. Use ONLY when the user explicitly requests landing.
---

If uncommitted changes remain, follow commit skill.
Create a task branch if HEAD is detached.
Rebase onto local main, resolving conflicts while preserving both branches' intended changes,
then merge into its checkout with `git merge --ff-only --no-autostash`.
Repeat if main gains new commits.

Keep checks proportional and reuse valid results already obtained.

Main may hold another session's work; merge around it if Git permits, never stashing,
committing, or discarding it. If it blocks, leave main alone.

Leave the branch and worktree in place; do not push. The host recreates and cleans up on its own.

Briefly report landed or queued, branch/commits, and checks.
