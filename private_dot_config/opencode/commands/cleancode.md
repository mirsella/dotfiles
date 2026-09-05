---
name: cleancode
description: Aggressively refactors recently touched code for simplicity, locality, robustness, and runtime efficiency while preserving intended behavior.
---

Aggressively improve all code you recently touched in this session, including
committed changes. Refactor related supporting code when it completes a worthwhile
improvement. Preserve other contributors' work.

Preserve intended behavior, not existing structure. Change module boundaries,
helper layers, APIs, data shapes, names, and call patterns when that improves the
code. Apply worthwhile improvements rather than merely suggesting them or stopping
at cosmetics. A coherent local refactor is better than a minimal patch that leaves
the design awkward. Do not churn equivalent code for stylistic preference.

## Simplicity and design

- Prefer fewer concepts, fewer moving parts, and direct, concrete, idiomatic code.
  Correctness, robustness, and meaningful efficiency matter more than cosmetic
  simplicity or saving implementation effort.
- Prefer designs that make code unnecessary. Actively look for entire branches,
  stored fields, conversions, parameters, and layers that can disappear rather
  than merely shortening their implementation. Judge the complete solution,
  including callers and supporting code: moving complexity elsewhere is not
  simplification. Less code is better when it preserves clarity, robustness,
  and meaningful performance.
- Fix underlying design problems instead of adding hacks, local workarounds,
  monkey patches, hidden second implementations, or layers of patches. Rewrite
  awkward control flow, collapse layers, merge fragmented logic, remove redundant
  state, and delete dead code.
- Use abstractions, generics, traits, combinators, and helpers when they reduce
  conceptual load, remove meaningful duplication, encode an invariant, clarify
  call sites, or provide useful performance. Keep them locally understandable.
- Concise code, readable one-liners, and local cleverness are welcome when their
  meaning is obvious and they do not combine unrelated effects. Avoid nonlocal
  magic and code that is compact only because it is harder to read.
- When branches differ only in a value or small decision, choose that value once
  and share the common operation instead of repeating the surrounding sequence.
  For example, choose the timeout first, then share request construction and error
  handling. Use early returns to remove unnecessary nesting. Keep genuinely
  different behaviors explicit rather than forcing them through a flag-heavy
  shared helper.
- Let names and structure explain straightforward operations. Remove comments
  that merely narrate the code, stale explanations, and redundant section labels.
  Keep useful API documentation and explanations of intent, invariants, surprising
  constraints, and performance-sensitive choices.

## Locality

- Keep behavior, state, validation, and side effects close together. Minimize the
  files, modules, traits, plugins, hooks, services, and callbacks a reader must
  follow to understand one behavior.
- Put behavior where the domain concept naturally belongs, not automatically on
  whichever type stores the data. Move it across type or module boundaries when
  the new owner is more appropriate.
- Inline pointless wrappers. Inline or merge single-use helpers, modules, traits,
  plugins, and layers unless separation improves readability, protects an
  invariant, or provides useful performance.
- If a helper belongs to one function, prefer keeping it inside that function as
  a local closure or local function. If it is called only once, prefer inlining
  it entirely unless extraction makes the code clearly easier to read. Neither
  approach should duplicate an existing domain rule.
- Prefer clear ownership and direct data flow over machinery introduced to work
  around an awkward design. Look for opportunities to borrow for temporary access,
  move when transferring ownership, and keep mutation with its owner instead of
  adding clones, shared ownership, locks, or interior mutability without a real
  need.

## Canonical rules and APIs

- Reuse existing domain APIs instead of re-deriving classification, validation,
  parsing, pricing, authorization, or state-transition rules. Even a slightly
  inconvenient method is better than rebuilding its internals. Prefer one named
  canonical implementation, not an existing method plus a local reconstruction.
- Never introduce a local closure that reimplements part of an existing trait
  method, enum method, parser, classifier, validator, or policy function. Locality
  means making the canonical rule easy to find, not copying it into callers.
- Avoid preflight checks that inspect a callee's internals or revalidate invariants
  its API owns. If the caller cannot handle failure differently and the callee
  already validates, logs, or errors appropriately, call it directly instead of
  checking `is_none`, `is_err`, type shape, capability flags, or optional subfields
  first.
- Validate locally for domain-specific recovery, a better typed error, preventing
  an invalid side effect, or avoiding substantial unnecessary work. Reuse canonical
  predicates rather than rebuilding the rule.
- A little incidental duplication is better than a bad abstraction. Duplicated
  domain policy is a different problem.

## Robustness and errors

- Prefer types, schemas, explicit validation, and clear invariants that make
  invalid states impossible or obvious. Avoid fragile invariants, hidden coupling,
  untracked temporary behavior, and code that handles only the current narrow
  case while pretending to be general.
- Choose data representations that remove bookkeeping and special cases. Prefer
  one meaningful state over mutually exclusive booleans such as `is_loading`,
  `is_ready`, and `has_failed`, and `Option<T>` over a separate presence flag plus
  a placeholder value. Derive cheap facts from their source instead of storing
  and synchronizing them. Keep cached or denormalized data when it avoids
  meaningful work.
- When something "shouldn't happen", do not silently continue. Return typed errors
  or explicit failure states in core logic. At side-effect boundaries, log
  `warn`/`error` when dropping, skipping, retrying, or recovering from unexpected
  input, without double-logging.
- Preserve useful diagnostics and deliberate recovery. Do not hide failures behind
  broad catch-alls, permissive parsing, silent fallbacks, or hidden alternative
  paths merely to make errors disappear.

## Delete obsolete structure

This is a greenfield project. Unless explicitly required, remove stale
compatibility code, migration shims, fallback adapters, parallel legacy
implementations, redundant configuration, dead branches, and unnecessary
indirection. Update affected callers instead of keeping compatibility glue or
obsolete APIs. Required temporary compatibility should be minimal, local, and tied
to a clear deletion condition. Deliberate recovery, supported platform
implementations, and useful performance fast paths are not obsolete compatibility.

Remove flexibility that serves no current purpose: unused parameters, mode flags
that never vary, configuration for fixed decisions, and extension points for
hypothetical implementations. Express the actual supported behavior directly
instead of maintaining a framework around it.

Prefer deletion and reuse over adding helpers, types, protocol shapes, or
configuration. Reduce net added code where practical, but do not chase a negative
diff or shrink code by hiding errors, removing useful diagnostics, or making
control flow denser.

## Performance and efficiency

Actively improve efficiency by doing less work. Prefer better algorithms and
eliminate unnecessary allocations, cloning, copies, repeated computation or
traversal, I/O, and synchronization. Consider the relevant workload, input sizes,
call frequency, ownership, and both runtime and memory costs.

Keep data in a useful representation. Remove unnecessary round trips through
strings or serialization, conversions between equivalent internal structs, and
temporary collections that only feed the next operation. Convert at meaningful
boundaries and materialize data when it provides a real benefit.

Keep modest local complexity when it buys meaningful performance. Do not replace
`SmallVec` with `Vec` merely because it looks simpler when that gives up useful
allocation behavior. Apply the same judgment to buffer reuse, caching, batching,
and data layout. Simplify around useful optimizations rather than deleting them.
Remove unnecessary specialization when justified; do not add machinery for
speculative gains or trade a substantial performance benefit for cosmetic cleanup.

## Finish

Follow repository requirements; otherwise keep checks lightweight. Routine Rust
cleanup usually needs only `cargo fmt` and/or scoped `cargo clippy`. Use focused
tests for uncertain or behavior-sensitive changes and test edits; measure
performance when a consequential tradeoff needs evidence.

Briefly report the improvements, checks actually run, and Git diff line counts,
total and per logical change, as `+added / -deleted (net +/-N)`, with the comparison
base clearly labeled.
