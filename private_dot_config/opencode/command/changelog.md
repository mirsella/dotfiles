---
description: Generate team changelog from git commits
tags: [changelog, release, git]
---

Generate a changelog for non-technical team members from one or more version ranges.

RULES:
1. Changes BELONG to a version if they come AFTER that version's bump commit
   - Example: Changes after "bump to 41.0.0" are in Version 41.0.0
   - Changes after "bump to 41.1.0" are in Version 41.1.0

2. For each version range:
   - Find the "bump to <version>" commits to determine boundaries
   - Get all commits between version bump commits in chronological order

3. Filter OUT (omit these completely):
   - cargo fmt, cargo update, pnpm update, npm update
   - "bump to", "bump nightly", version bumps
   - "remove .json from .gitignore", "add .sqlx folder"
   - "cap sync", "try_despawn", "fix missing .sqlx folder"
   - Pure backend router fixes (unless user-facing)
   - Code refactoring without user-facing changes
   - Internal code organization changes

4. Filter IN (include these):
   - New features (tutorial, leaderboard, timer, etc.)
   - Bug fixes
   - UI improvements and visual changes
   - Performance improvements (asset loading, compression)
   - Security updates
   - Gameplay changes (bot behavior, card mechanics)

5. Attribution:
   - Check author name for each commit
   - If author is "enz000" (not mirsella), prefix with "Enzo: "
   - Include "Enzo:" in PR titles where he contributed

6. Formatting:
   - Start with "## Version X.Y.Z (YYYY-MM-DD)" header
   - List all changes as bullet points
   - Use plain English descriptions (not commit hashes)
   - Deduplicate similar entries (e.g., multiple "update castle assets")
   - Sort versions oldest first, latest last
   - NO overall "Changelog" title or file header

$ARGUMENTS
