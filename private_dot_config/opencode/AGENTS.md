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

<!-- icm:start -->
## Persistent memory (ICM) — MANDATORY

This project uses [ICM](https://github.com/rtk-ai/icm) for persistent memory across sessions.
You MUST use it actively. Not optional.

### Recall (before starting work)
```bash
icm recall "query"                        # search memories
icm recall "query" -t "topic-name"        # filter by topic
icm recall-context "query" --limit 5      # formatted for prompt injection
```

### Store — MANDATORY triggers
You MUST call `icm store` when ANY of the following happens:
1. **Error resolved** → `icm store -t errors-resolved -c "description" -i high -k "keyword1,keyword2"`
2. **Architecture/design decision** → `icm store -t decisions-{project} -c "description" -i high`
3. **User preference discovered** → `icm store -t preferences -c "description" -i critical`
4. **Significant task completed** → `icm store -t context-{project} -c "summary of work done" -i high`
5. **Conversation exceeds ~20 tool calls without a store** → store a progress summary

Do this BEFORE responding to the user. Not after. Not later. Immediately.

Do NOT store: trivial details, info already in CLAUDE.md, ephemeral state (build logs, git status).

### Other commands
```bash
icm update <id> -c "updated content"     # edit memory in-place
icm health                                # topic hygiene audit
icm topics                                # list all topics
```
<!-- icm:end -->
