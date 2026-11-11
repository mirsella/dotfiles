---
name: finish
description: Finish this session's work in one turn: do its remaining follow-ups, clean up, review and fix, then report what is left. Arguments `land` and `settle` are optional. Use ONLY when the user explicitly requests finishing.
---

Bring this session's work to a finished state in one turn. This replaces
running the recommendations, cleancode and reviews skills one after another and
answering "do all" between them. Load those skills and follow them for their own
steps; this skill sets only the order, the defaults and the ending.

$ARGUMENTS

## Steps

1. **Follow-ups first.** Work out what the recommendations skill would list for
   this session's work, then do it now: everything necessary, and the optional
   items that are small and clearly part of the task. Doing them first lets the
   cleanup and the review cover the whole change. Leave for the final message
   anything that needs the user's decision (product behavior, a materially
   different design, a broken contract, a data migration), is a large separate
   piece of work, or needs access you lack.
2. **Clean up.** Apply the cleancode skill to everything this session touched,
   including the follow-ups from step 1.
3. **Review and fix.** Apply the reviews skill to the whole change and fix what
   it finds under that skill's rules.
4. **Repeat only when warranted.** If the fixes from step 3 changed behavior in
   a non-trivial way, have those fixes reviewed once more; with a Sol reviewer
   that is a later round of the same loop, within its budget. Do not repeat the
   cleanup, and do not start a third review. Repeat passes over unchanged code
   find almost nothing.

Run the smallest checks that cover the final state once, at the end, following
the repository's rules. Reuse earlier results that later edits did not
invalidate.

## Arguments

- No argument: stop after the steps. Do not commit, push or land.
- `land`: after the steps, follow the land skill. Do not settle the thread.
- `settle`, together with `land`: after landing, settle the thread only if every
  included project landed and nothing important is left to do. If something
  important is left, do not settle, and say what it is.

Any other text narrows or adjusts the work.

## Final message

The user runs many threads at once and can miss a follow-up buried in a long
report, so keep this message short and end it the same way every time:

- What this turn changed, in a few sentences, with one `checks:` line and one
  `diff:` line for the whole turn in the format the reviews skill defines.
- With `land`: the land skill's report for each project. The section below
  replaces its closing recommendations.
- Last, under the heading **Left to do**: every follow-up you did not do, most
  important first, each with the action needed and why you left it. If there are
  none, write "Nothing left to do: this work is complete." Committing, pushing,
  landing and settling are not follow-ups.
