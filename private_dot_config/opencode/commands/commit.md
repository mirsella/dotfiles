---
description: Git commit
model: openai/gpt-5.6-luna#max
---

Use the existing conversation context to identify the changes performed by this session that have not yet been committed. Stay in this session; do not delegate to a subagent.
Inspect `git status`, `git diff`, `git diff --cached`, and `git log --oneline -10` before staging or committing.
The worktree and index may contain changes from other agents. Stage and commit only this session's changes, identified from the conversation and confirmed against the diff. Never assume all changes since the last commit belong to this session.
Use explicit paths for files wholly owned by this session. For files containing mixed changes, stage only this session's hunks with a non-interactive patch. Do not use `git add .`, `git add -A`, or `git commit -a`.
Preserve unrelated worktree and staged changes. If unrelated changes are already staged, use a temporary index to build a commit containing only this session's changes, and reconcile only the committed hunks in the real index. If ownership or safe isolation is unclear, ask rather than include or overwrite another agent's work.
Review the exact staged diff that will be committed before committing. Never include secrets.
When multiples things were done, dont hesitate to commit separately, to have smaller, cleaner, logical commits.
Generate a conventional commit message following the format:

Format: <type>(<scope>): <subject>

Careful of correctly handling backtick when running bash commands, use single quotes so there's no command substitution.

The commit title should contains what was done. the description, if any, should also contains the motivation. (for example, fixing a bug)
Add detailed body if changes are substantial.
Commit the changes with the generated message.
Your only job is to commit code changes to git. dont propose changes.
Dont go on fixing other things. your only job this turn is to commit the current code.
Do not run repository-wide formatters or include unrelated formatting changes.

$ARGUMENTS
