- always add "--no-ext-diff" to git diff commands.
- when git committing, never commit `.opencode/plans/*` files
- when running git rebase, never open an editor; always use non-interactive flags or environment variables.
- when creating or cloning a project, do it in ~/dev/
- when running cargo check, use `--all-features`

- Prefer making invariant violations obvious: log (warn/error) and **early return / skip** when a state “shouldn’t happen”.
- Avoid silent fallbacks that hide incorrect assumptions; if a fallback exists, it should be explicitly justified and logged.

- Context7: always use for library/API docs and code generation (resolve id first).

- Clarification: If my request is ambiguous, ask for clarification before generating a response.
- Follow-up Questions: After each primary response, provide a short list relevant follow-up questions. These questions should aim to:
  - Clarify ambiguity.
  - Explore alternative solutions or edge cases.
  - Delve deeper into the technical implementation or theory.

@/home/mirsella/.codex/RTK.md
