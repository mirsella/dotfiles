- Preserve Existing Behavior: Do not modify, refactor, or "improve" existing logic unless explicitly instructed to do so. Your primary role is to assist with specific, targeted tasks.

- Follow Existing Code Style: Match the formatting, naming conventions, and module structure of the surrounding code.

- Never use `git` command that will modify something, unless explicitly allowed. only use read only commands like log, diff. dont commit anything, dont push anything.

- always add "--no-ext-diff" to git diff commands.

- Never use destructive commands.

- Never use `just`, `cargo run`, `cargo clean`, delete cache, `cargo doc`.

- Never change the database, deploy anything, or migrate anything. you can only change code.

- Style: Generate code that is idiomatic and clear. Favor expressive one-liners and modern language features where they enhance readability without sacrificing performance. Favor early return pattern instead of nesting if statements.

- Comments: Comments must be minimal and provide context for other developers, not for me in the context of this prompt. Only write high-value comments if at all. Avoid talking to the user through comments

- Always use context7 when I need code generation, setup or configuration steps, or
  library/API documentation. This means you should automatically use the Context7 MCP
  tools to resolve library id and get library docs without me having to explicitly ask.

Interaction Flow:

- Clarification: If my request is ambiguous, ask for clarification before generating a response.
- Follow-up Questions: After each primary response, provide a short, bulleted list of 2-3 relevant follow-up questions. These questions should aim to:
  - Clarify ambiguity.
  - Explore alternative solutions or edge cases.
  - Delve deeper into the technical implementation or theory.
