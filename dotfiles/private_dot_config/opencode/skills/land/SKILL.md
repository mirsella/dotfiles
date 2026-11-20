---
name: land
description: Commit and land only this agent's own changes from the current session across all projects it changed, or queue blocked work. Use ONLY when the user explicitly requests landing.
---

Land only this session's own changes, including work it delegated, in every
project it changed unless the user narrows the scope. Read-only visits do not
count. Leave other sessions' work untouched without asking about it; if
ownership is unclear, exclude the change.

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

Settle only when requested and all included work is landed, as the last tool
call of the turn. First make sure nothing will wake the thread: every delegate,
subagent and background command has finished and been handled (cancel unneeded
ones; prefer `mode: "wait"`). If something still runs, settle in the turn its
completion starts instead. If woken after settling by a notice needing no work,
settle again.

Briefly report each project's landed commits, checks, and skipped, retained or
blocked work. Do not claim full landing after partial success.
End by following the recommendations skill for what is still left to do on the
task itself.
