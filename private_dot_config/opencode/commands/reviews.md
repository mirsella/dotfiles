---
description: Review session changes and fix important defects; ask before substantial fixes
agent: build
---

Review the code you changed in this session, including committed changes and
work you delegated. Focus on concrete defects introduced by this work that are
worth fixing before merge, not style preferences, speculative hardening, or
pre-existing issues. Preserve other contributors' edits.

Additional focus: $ARGUMENTS

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

Keep checks lightweight: usually formatting and scoped Clippy, with focused
tests or measurements for sensitive or uncertain changes.

Include Git diff line counts, total and per logical fix, as
`+added / -deleted (net +/-N)`, with the comparison base labeled.
