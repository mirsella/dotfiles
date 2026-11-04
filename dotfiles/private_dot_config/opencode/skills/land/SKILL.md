---
name: land
description: Commit and land only this agent's own changes from the current session across all projects it changed, or queue blocked work. Use ONLY when the user explicitly requests landing.
---

Land only changes made by this agent in the current thread/session, including
work it explicitly delegated for this task. Include all projects where it made
those changes unless the user narrows the scope. Read-only visits do not count.

Inspect this session's own changes and task branches/worktrees for unlanded
work. Check whether its changes already landed; do not inventory unrelated
branches or worktrees as additional landing candidates.

Other sessions' work is out of scope even in the same project or checkout.
Leave it untouched without asking whether to review, include or land it, unless
the user explicitly requests that scope. If ownership is uncertain, exclude the
change; ask only if that uncertainty prevents isolating or landing this agent's
own changes.

For each included project:
- Follow commit skill for uncommitted changes.
- Create a task branch if HEAD is detached.
- Rebase onto local main, preserving both branches' intended changes,
  then merge into its checkout with `git merge --ff-only --no-autostash`.
- Repeat if main advances, then push main. Respect dependencies between projects.
- Preserve other sessions' work. Merge around it if Git permits; never stash,
  commit or discard it. If blocked, leave it untouched and report the blocker.

Keep checks proportional and reuse valid results. Deployment requires a
separate request.

After successful landing and pushing, clean up temporary manual worktrees:
- Leave T3-managed worktrees to T3 settlement and use other tools' own lifecycle.
  Identify management from metadata, not directory names alone.
- Remove only worktrees created by this session, with all work landed/pushed,
  no remaining changes or valuable untracked/ignored files, and no active use.
- Move outside the worktree first; use `git worktree remove` without
  force, never recursive deletion.
- Retain worktrees when ownership, contents or active use are uncertain.
  Preserve primary checkouts, branches and worktrees containing skipped work.

Settle the thread only when requested, all included work
is landed and its worktrees are clean. Never discard work to enable settlement.

Briefly report each project's landed commits, checks, removed worktrees, and
skipped, retained or blocked work. Do not claim full landing after partial success.
