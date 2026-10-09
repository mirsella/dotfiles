---
description: Review only code changed in this session by this agent and fix important defects; ask before substantial fixes. Use when the user asks to review session changes.
agent: build
---

Review only the code changed in this session by this agent, including committed
changes and work you delegated. Focus on concrete defects introduced by this
work that are worth fixing before merge, not style preferences, speculative
hardening, or pre-existing issues.
Other agents may be working in the same worktree — only review your own changes, never theirs. If ownership of a hunk is shared or uncertain, exclude it. Formatting is fine.
Delegate read-only reviews to subagents for fresh context. Give them the owned
diff, relevant context, and existing verification results. They should report
concrete defects and verification gaps without editing files or running builds,
linters, or tests. The parent agent owns fixes and any needed verification.

$ARGUMENTS

## Fix by default

Choose and apply the best fix without asking about routine implementation
choices. Include related supporting code and affected callers when needed to
fix the problem properly.

Fix causes rather than layering workarounds. Prefer deletion, canonical APIs,
and local behavior without sacrificing meaningful performance. Refactor as
needed for the fix, not as a separate cleanup pass.

## Ask before big fixes

Ask before substantial fixes or consequential decisions, such as broad
refactors, major design changes, breaking external contracts, data migrations,
or choosing between materially different intended behaviors. Explain your
recommendation and wait for approval, even when the best option seems clear.

## Verification and reporting

Review code by default; this command does not automatically run formatting,
Clippy, builds, or tests. Reuse valid results from implementation. After fixes,
follow repository requirements and run only the smallest checks needed for the
affected behavior or a concrete uncertainty. Finish the fixes before checking;
do not repeat a successful check unless later changes invalidate it. A release
gate is not a routine review step.
Report performed checks, reused results, or review-only verification in a single
line: "checks: ". State any remaining verification gap.

Without jumping a line, briefly report improvement with one line: `diff: Total <counts> | Code <counts> | Tests <counts>`.
Counts are `+/-net (+added/-deleted)`, with only net numbers bold. Count only your
edits in this turn; Tests includes inline tests and fixtures, Code everything else.
