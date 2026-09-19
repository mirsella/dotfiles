---
description: Git commit
model: openai/gpt-5.6-luna#max
---

Use the existing conversation context to identify the changes performed by this session that have not yet been committed. Stay in this session; do not delegate to a subagent.
The worktree and index may contain changes from other agents. Stage and commit only this session's changes, identified from the conversation and confirmed against the diff. Never assume all changes since the last commit belong to this session.
Review the exact staged diff that will be committed before committing. Never include secrets.
When multiples things were done, dont hesitate to commit separately, to have smaller, cleaner, logical commits.
Generate a conventional commit message following the format:
Careful of correctly handling backtick when running bash commands, use single quotes so there's no command substitution.
The commit title should contains what was done. the description, if any, should also contains the motivation. (for example, fixing a bug)
Add detailed body if changes are substantial.
Dont go on fixing other things. your only job this turn is to commit the current code.
Run formatters for touched code like cargo fmt and include formatting changes in the same commit if related, or in a new commit just for the fmt changes.
If we have worked on a fork or dependencies, this means also committing and pushing them, updating the lockfile, and committing the lockfile changes.

$ARGUMENTS
