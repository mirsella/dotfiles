---
name: cleancode
description: Refactors only this agent's current-session changes for simplicity, correctness, and runtime efficiency while preserving intended behavior.
---

Refactor the code you changed in this session into simpler, idiomatic
code that does less work. Apply worthwhile improvements, not just
suggestions. Do not stop at cosmetic cleanup when a deeper simplification
is clearly justified within scope.

## Scope and ownership

- Before editing, identify all still-present code you created or modified
  in this session, including earlier turns, committed edits, and work you
  explicitly delegated. Do not limit the pass to your last patch.
- Establish ownership from session context and edit history. Git diffs
  help inspect changes; they do not prove ownership. A dirty worktree or
  a touched file is not blanket permission to edit it.
- Keep the workset to the specific functions or blocks you changed.
  Preserve the user's and independent agents' edits, including overlapping
  changes. When ownership is uncertain, leave that code unchanged and
  report the limitation.
- The scope stays fixed during cleanup. Reading related code or making
  incidental edits does not expand it. Do not refactor untouched callers
  or helpers. If a sound change requires out-of-scope edits, leave it
  unapplied and report the dependency rather than adding a workaround.
  Delegated agents inherit the same limits.

## Priorities

Correctness, safety, and required behavior are constraints. Preserve
meaningful performance before optimizing for fewer concepts, clearer
code, or fewer lines. Prefer the simplest design that preserves or
improves efficiency for the relevant workload.

Net diff size is a tie-breaker, not a target. Prefer deletion and reuse
when otherwise comparable, but accept added code or tests when justified.
Never chase a negative diff, compress control flow, hide errors, or remove
useful diagnostics to reduce line count.

## Refactoring rules

- Follow repository instructions, toolchain, and idioms. Preserve
  task-defined behavior, side effects, error handling, and supported
  contracts. Do not introduce features or silently reinterpret requirements.
- Remove dead code, redundant state, pointless wrappers, obsolete
  configuration, and unnecessary indirection within scope. Simplify
  awkward control flow and collapse fragmented logic. Prefer a coherent
  fix over layers of local patches.
- Reuse the canonical domain API instead of reconstructing its rules
  locally. Do not duplicate validation, parsing, classification, policy,
  or state-transition logic in a helper or closure. Keep related behavior
  close without copying rules into callers.
- Do not make callers inspect a callee's internals or repeat checks
  already handled by its API. Keep local checks for distinct recovery,
  better typed errors, preventing invalid side effects, or avoiding
  substantial work the API would otherwise perform. Reuse canonical
  predicates rather than duplicating their logic.
- Prefer direct, concrete code. Keep a helper, type, trait, or layer when
  it names meaningful work, enforces an invariant, isolates a real
  boundary, or provides a worthwhile performance benefit. Inline trivial
  forwarding and needless fragmentation. Single use alone does not make
  an abstraction unnecessary; similar-looking code alone does not justify
  sharing one.
- Make invalid states impossible or explicit. Preserve useful typed
  errors and deliberate recovery. Do not swallow unexpected failures,
  add permissive fallbacks, or log the same failure at multiple layers.
  Log where the failure is handled with useful context, not on every
  propagation step.
- Treat this project as greenfield unless repository or task instructions
  say otherwise. Remove obsolete compatibility and transitional
  implementations in scope; do not add shims or parallel legacy
  implementations without an explicit requirement. This does not
  authorize removing deliberate error recovery, supported platform
  implementations, or performance fast paths. Explicitly required
  temporary compatibility must stay minimal, local, and have a clear
  deletion condition.

## Performance and efficiency

- Prefer a better algorithm or doing less work over micro-optimizing the
  same work. Remove unnecessary computation, allocations, cloning,
  copies, repeated traversals, I/O, and synchronization. Consider actual
  input sizes, call frequency, and ownership rather than only the edited
  function in isolation.
- Do not accept material regressions in relevant latency, throughput,
  allocation behavior, memory use, or other resource costs for cosmetic
  simplicity. Keep modest local complexity when it avoids meaningful
  cost. Do not add speculative machinery for theoretical gains.
- Treat changes such as `SmallVec` to `Vec`, removing buffer reuse or
  caching, undoing batching, or changing data layout as performance
  decisions, not style cleanup. Understand why the current choice exists
  before replacing it. Neither a specialized implementation nor a
  simpler-looking replacement is automatically better.
- Preserve useful optimizations while simplifying their surrounding
  code. Remove specialization only when evidence shows it is unnecessary
  or a replacement introduces no meaningful regression for the actual
  workload.
- Use direct cost reasoning when conclusive. For consequential, uncertain
  tradeoffs, compare before and after under the same representative
  workload when feasible. Do not demand benchmarks for every mechanical
  cleanup. When evidence is insufficient, retain the current
  performance-sensitive choice rather than guessing. Distinguish
  cost-based reasoning from measured results; passing correctness tests
  does not prove performance equivalence.

## Verify and finish

- Review the entire identified workset, with enough surrounding code and
  tests to understand its behavior and invariants.
- Run the relevant formatter, build or type checks, lints, and tests
  using repository conventions. Add or update focused tests when needed
  to protect scoped behavior. Do not let formatting, generated files, or
  test cleanup introduce unrelated changes.
- Inspect the final diff for scope violations and unintended behavior or
  performance changes. Do not label failures pre-existing without
  evidence. State any checks you could not run.
- Stop when the whole workset has been reviewed and worthwhile, justified
  improvements are complete. Already-clean code may remain unchanged.
  Do not manufacture changes or continue merely because the diff is
  net-positive.
- Finish with a brief summary of changes, checks actually run, and any
  performance-sensitive decisions or blocked work. Do not claim coverage
  of code you could not identify or inspect.
