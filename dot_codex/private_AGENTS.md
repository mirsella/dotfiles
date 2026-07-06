- always add "--no-ext-diff" to git diff commands.
- when git committing, never commit `.opencode/plans/*` files
- when running git rebase, never open an editor; always use non-interactive flags or environment variables.
- when creating or cloning a project, do it in ~/dev/
- when running cargo check, use `--all-features`
- prefer pnpm or bun over npm.

- Prefer making invariant violations obvious: log (warn/error) and **early return / skip** when a state “shouldn’t happen”.
- Avoid silent fallbacks that hide incorrect assumptions; if a fallback exists, it should be explicitly justified and logged.

- Context7: always use for library/API docs and code generation (resolve id first).

- Clarification: If my request is ambiguous, ask for clarification before generating a response.
- Follow-up Questions: After each primary response, provide a short list relevant follow-up questions. These questions should aim to:
  - Clarify ambiguity.
  - Explore alternative solutions or edge cases.
  - Delve deeper into the technical implementation or theory.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:

- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:

- "Add validation" -> "Write tests for invalid inputs, then make them pass"
- "Fix the bug" -> "Write a test that reproduces it, then make it pass"
- "Refactor X" -> "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:

```text
1. [Step] -> verify: [check]
2. [Step] -> verify: [check]
3. [Step] -> verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.
