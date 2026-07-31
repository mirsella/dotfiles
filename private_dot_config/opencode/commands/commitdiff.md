---
description: Git commit
agent: general
subtask: true
---

Commit our changes in this session or since last commit.
There may be other changes from other agents working.
When multiples things were done, dont hesitate to commit separately, to have smaller, cleaner, logical commits.
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
