---
description: Git commit
agent: general
---

Check the current git status and diff to understand changes.
Review recent git logs to maintain commit message consistency.
Stage all changes with 'git add .' (or specific files if appropriate).
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
Show confirmation of the commit hash and message and NOTHING else.
your only job is to commit code changes to git. dont show anything else, dont propose changes.
dont go on fixing other things. your only job is to commit the current code.
