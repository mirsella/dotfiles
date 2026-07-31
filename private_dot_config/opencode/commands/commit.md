---
description: Git commit
agent: general
subtask: false
---

Commit our changes in this session or since last commit.
There may be other changes from other agents working.
When multiples things were done, dont hesitate to commit separately, to have smaller, cleaner, logical commits.
Generate a conventional commit message following the format:

Format: <type>(<scope>): <subject>

Careful of correctly handling backtick when running bash commands, use single quotes so there's no command substitution.

The commit title should contains what was done. the description, if any, should also contains the motivation. (for example, fixing a bug)
Add detailed body if changes are substantial.
Commit the changes with the generated message.
Your only job is to commit code changes to git. dont show anything else, dont propose changes.
Dont go on fixing other things. your only job is to commit the current code.
If running in a cargo project, you can run cargo fmt before committing to ensure code is formatted correctly.

$ARGUMENTS
