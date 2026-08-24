---
description: Review only current-session changes and fix clear, local merge-blocking bugs
agent: build
---

Review only the code changed by the OpenCode session that invoked this command. Find clear defects that should block the work from being merged. Fix only small, obvious defects. Ask before attempting anything substantial.

Additional focus: $ARGUMENTS

## Scope and ownership

The current conversation and tool history are the source of truth for ownership.

Before reviewing, reconstruct and freeze the exact set of hunks created, modified, deleted, moved, or renamed by this session, including work performed by subagents launched from this session.

- Ownership is determined per hunk, not per file.
- Use Git to inspect changes already known to belong to this session, not to determine ownership.
- Do not infer ownership from `HEAD`, the branch diff, working tree, index, untracked files, commits, timestamps, or nearby changes.
- `$ARGUMENTS` may narrow the review or name an area of concern. It never expands the review beyond this session's changes.
- Exclude any hunk whose ownership is shared or uncertain.
- If this session has no reviewable changes, say so and stop. Do not review `HEAD`, the branch, or the working tree as a fallback.

Other agents may be working in the same worktree. Never review, fix, revert, reformat, stage, commit, or otherwise alter their work.

Read the owned hunks and only enough surrounding code, callers, contracts, and tests to understand their behavior. Do not turn this into an audit of entire affected files or nearby code.

A finding must be caused by a change owned by this session. The resulting failure may appear elsewhere, but the causal change must be inside the frozen review target.

Before starting the review, state:

`Reviewing: <session-owned files/hunks>; all other worktree changes excluded.`

## Finding threshold

Report a finding only when all of these are true:

- This session introduced it.
- The evidence supports it. Investigate uncertainty instead of guessing.
- It has a realistic trigger, not merely a hypothetical one.
- It can cause incorrect behavior, a crash, data loss or corruption, a security problem, a broken external contract, or a material performance regression.
- It is important enough to fix before merge.

Do not report:

- Style or naming preferences
- Cleanup opportunities
- Possible refactors
- Preferred architecture
- Speculative hardening
- Defensive code without a demonstrated need
- Missing tests or documentation unless they directly demonstrate a qualifying defect
- Pre-existing problems
- Problems introduced by another session
- Low-confidence or low-value concerns

Do not pad the review with suggestions, caveats, or hypothetical edge cases. If a finding does not clearly meet the bar, omit it.

## Process

1. Reconstruct and freeze the session-owned patch.
2. Read the applicable repository instructions, the owned changes, and only the context needed to understand them.
3. Review the patch yourself.
4. For a non-trivial or high-risk patch, send the frozen patch, necessary context, and this review bar to one independent review subagent. Use a second independent reviewer only when the patch spans separate high-risk areas where another perspective is useful.
5. Delegated reviewers are strictly read-only. Tell them not to edit files, run mutating commands, stage changes, or otherwise modify the worktree.
6. Verify every reported issue yourself. Remove duplicates, weak claims, speculative findings, and anything outside the frozen scope.
7. Apply only fixes allowed by the automatic-fix rules below.
8. Run the smallest targeted checks that demonstrate the affected behavior still works.
9. Inspect the edits made by this review and confirm that no other session's work was changed.

Do not increase the number of findings or reviewers merely to appear thorough.

If a check fails for a reason outside the frozen review target, mention it only as a verification limitation. Do not investigate or fix that unrelated failure.

## Concurrent work

Assume the worktree may change while the review is running.

Immediately before editing a file, re-read the relevant area and confirm that the lines you intend to change still match the reviewed state.

If another session has modified the relevant hunk, or ownership has become uncertain:

- Do not overwrite, revert, merge, or clean up the concurrent change.
- Do not guess which version should win.
- Stop editing that hunk and report the conflict.

Unrelated concurrent changes elsewhere in the worktree are not part of this review.

## Automatic fixes

Fix an issue without asking only when every condition below holds:

- The defect clearly meets the finding threshold.
- The correct implementation is obvious.
- No product, design, compatibility, or architecture decision is required.
- The fix is local and directly addresses the defect.
- It changes only session-owned code, or inserts the minimum necessary new code inside the same owned area.
- It does not modify a shared or unowned line.
- The complete automatic fix touches at most two files.
- The complete automatic fix changes at most 20 lines, counting additions and deletions across production code and tests.
- It does not change a public API, schema, persisted data, dependency, configuration, or migration.
- It does not require a new abstraction, wrapper, compatibility layer, broad refactor, or unrelated cleanup.

Use the smallest direct fix.

Prefer, in order:

1. Correcting the faulty code
2. Deleting unnecessary faulty code
3. Reusing existing repository code
4. Using the standard library or native platform behavior
5. Using an already-installed dependency

Do not add code merely to make the implementation look more robust.

In particular, do not add:

- Speculative guards
- Catch-all error handling
- One-use abstractions or helpers
- Compatibility paths without a demonstrated requirement
- Configuration knobs
- Comments that restate the code
- Tests that do not reproduce or protect against the actual defect
- Unrelated cleanup while touching the area

Do not rewrite an entire file for a local fix.

Do not run a formatter, fixer, generator, snapshot update, or similar command when it may rewrite lines outside this session's owned changes.

## Larger fixes

If an important finding cannot be fixed under the automatic-fix rules, do not start implementing it.

Use the question tool and give the user a short decision report containing:

- What fails and what triggers it
- Severity and supporting evidence
- The smallest correct fix
- Which files it would affect
- Approximate size of the change
- Why fixing it now is worthwhile
- Why deferring or accepting it could be reasonable

Ask whether to:

- Apply the proposed fix
- Defer it
- Take another approach

Wait for the user's answer before making the change.

Always ask first if the fix:

- Requires a product or design decision
- Changes a public API, schema, or stored data
- Adds or changes a dependency
- Adds an abstraction, migration, compatibility path, or configuration
- Spans more than two files
- Changes more than 20 lines
- Is otherwise disproportionate to the defect

If the correct fix requires modifying shared or unowned code, do not modify that code as part of `/reviews`, even after identifying the problem. Explain what would need to change and ask how the user wants to proceed.

If the question tool is unavailable, present the decision report and stop.

Do not ask the user about non-blocking suggestions. Omit them.

## Result

Lead with any unresolved merge-blocking findings, ordered by severity and linked to the relevant session-owned file and line.

Then state briefly:

- What session-owned changes were reviewed
- What was fixed
- What checks were run and their results
- Any verification limitation or concurrent-edit conflict

If no qualifying issue remains, say:

`No merge-blocking issues found in the session-owned changes.`

Do not add compliments, filler, optional improvements, speculative suggestions, or unrelated observations.
