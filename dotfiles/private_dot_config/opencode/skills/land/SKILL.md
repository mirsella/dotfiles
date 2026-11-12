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
- Landing is this direct merge. Do not open or suggest a pull request unless asked.
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

Settling is the last action of the turn, because any later turn unsettles the
thread, including one started by a delegate's or background job's completion
notice:
- Before settling, make sure nothing will wake the thread. Every delegated task,
  subagent and background command has finished and its result has been handled.
  Cancel the ones no longer needed. If settling was requested from the start,
  run delegates with `mode: "wait"` where the tool allows it, so no completion
  notice arrives after the turn.
- If something is still running, do not settle. End the turn as its tool
  directs, and settle at the end of the turn its completion starts.
- Settle after every other tool call, then write the final report.
- If the thread wakes after settling to a notice that needs no work, such as a
  duplicate or an already-handled result, settle again and end the turn.

Briefly report each project's landed commits, checks, removed worktrees, and
skipped, retained or blocked work. Do not claim full landing after partial success.
End by following the recommendations skill for what is still left to do on the
task itself.
