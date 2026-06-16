---
description: Git commit
agent: general
subtask: false
---

Commit our changes for this session, or last time we made one.
No need to check the git diff, you know what files you modified.
Stage all files you changed with git add.
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
