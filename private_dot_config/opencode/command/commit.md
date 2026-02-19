---
description: Git commit
agent: general
model: google/antigravity-gemini-3-flash
subtask: false
---

Commit our changes for this session, or last time we made one.
No need to check the git diff, you know what files you modified.
Stage all files you changed with git add.
Generate a conventional commit message following the format:

- feat: for new features
- fix: for bug fixes
- refactor: for code refactoring
- chore: for maintenance tasks
- docs: for documentation changes
- style: for formatting changes
- test: for test additions/modifications
- perf: for performance improvements

Format: <type>(<scope>): <subject>

Careful of correctly handling backtick when running bash commands.

Add detailed body if changes are substantial.
Commit the changes with the generated message.
your only job is to commit code changes to git. dont show anything else, dont propose changes.
dont go on fixing other things. your only job is to commit the current code.
if running in a cargo project, you can run cargo fmt before committing to ensure code is formatted correctly.

$ARGUMENTS
