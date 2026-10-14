---
name: land
description: Commit and land this session's branch on main and push it, or queue it when another session blocks landing. Use ONLY when the user explicitly requests landing.
---

If uncommitted changes remain, follow
`~/.config/opencode/skills/commit/SKILL.md`. Create a task branch if HEAD is detached.

Then land the branch:
1. In main's checkout, `git pull --ff-only` to pick up remote commits.
   If it fails, leave main alone and report the branch as queued.
2. Rebase the branch onto main, resolving conflicts while preserving both
   branches' intended changes.
3. Merge into main with `git merge --ff-only --no-autostash`, then `git push`.
   A rejected push means main gained new commits: repeat from step 1.

Keep checks proportional and reuse valid results already obtained.

Main may hold another session's work; merge around it if Git permits, never
stashing, committing, or discarding it. If it blocks, leave main alone.

Leave the branch and worktree in place. The host recreates and cleans up on its own.

Briefly report landed or queued, branch/commits, and checks.
