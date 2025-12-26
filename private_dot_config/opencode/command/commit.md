---
description: Git commit
agent: general
subtask: true
model: google/gemini-2.5-flash
---

Use git commands to commit changes. DO NOT create a script.

Review all changes before committing. Ensure that the commit message is clear and descriptive
Arguments provided: "$ARGUMENTS"

If an argument is provided above:

- ONLY commit changes in that specific directory

- Do NOT check the current directory or other child directories

- ONLY process the single directory specified

If NO argument is provided above (empty string):

- Check if the current directory is a Git repository and commit changes with a message
- If the current directory is not a Git repository, check child directories for Git repositories and commit changes in each one

user arguments that should steer the commit message: "$ARGUMENTS"
