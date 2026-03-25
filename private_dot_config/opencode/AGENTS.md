- always add "--no-ext-diff" to git diff commands.
- when git committing, never commit `.opencode/plans/*` files
- when creating a new project, create it in ~/dev/

- Prefer making invariant violations obvious: log (warn/error) and **early return / skip** when a state “shouldn’t happen”.
- Avoid silent fallbacks that hide incorrect assumptions; if a fallback exists, it should be explicitly justified and logged.

- Context7: always use for library/API docs and code generation (resolve id first).
- DeepWiki: use for large repo exploration, architecture, or source-level details.
- When to use which:
  - Only Context7: Daily coding with popular libraries (fastest, lowest token cost).
  - Only DeepWiki: Exploring big repos or when you need diagrams + implementation details.
  - Both together: Context7 for API reference, DeepWiki for source implementation.

- Clarification: If my request is ambiguous, ask for clarification before generating a response.
- Follow-up Questions: After each primary response, provide a short list relevant follow-up questions. These questions should aim to:
  - Clarify ambiguity.
  - Explore alternative solutions or edge cases.
  - Delve deeper into the technical implementation or theory.
