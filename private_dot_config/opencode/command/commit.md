---
agent: build
description: Commit staged changes with a conventional commit message
---

opencode Generate a commit message and perform the commit following these rules explicitly:

Analyze the changes and generate a proper commit message.

Use Conventional Commits v1.0.0 format with this structure:
<type>(optional scope): <short summary>

<detailed bullet points (one per line)>

Use only these prefix types:

- feat: New feature or functionality
- fix: Bug fix
- docs: Documentation-only changes
- style: Code style changes (no logic change)
- refactor: Code changes neither fixing bug nor adding feature
- perf: Performance improvements
- test: Adding or correcting tests
- build: Changes to build system, dependencies, CI/CD
- chore: Maintenance, tooling, config
- ci: CI configuration and scripts
- revert: Reverting a previous commit

Never invent new prefixes; use chore as fallback.

Scope: Include scope only if requested. Optional, lowercase, hyphenated (e.g., (auth), (ui/login)); reflect module/component/domain.

Short summary: Imperative present tense (add, fix, update), start lowercase, no period.

IMPORTANT: the total length of the first line (the summary line) must not exceed 50 chars and the total length of subsequent lines must not exceed 72 characters.

Detailed bullets: Start after blank line, one change per bullet, start with capital letter, use dash (-), no period unless multiple sentences, focus on what/why.

For breaking changes: Add "BREAKING CHANGE: <description>" at end after bullets.

AI rules: Never hallucinate - only reference actual changes; always >=1 bullet unless trivial; group related changes; for renames use refactor and mention old->new; for large changes summarize patterns; respect .gitignore.

Edge cases: Skip merge commits; for empty: chore: initial commit; version bump: chore: bump version to vX.Y.Z; deps: build(deps): update <pkg> from A to B; config: chore(config): update <rules>.

Output only the raw commit message text, no extras.

Self-check: Valid prefix, lowercase scope, summary rules, blank line, bullets format, no trailing whitespace, note breaking if applicable.

<UserRequest>
  $ARGUMENTS
</UserRequest>
