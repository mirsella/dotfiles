- Most projects are in `~/dev`; put clones, worktrees, and new projects there.
- `/tmp` is RAM-backed and consumes RAM or swap. Use `/tmp/opencode` only for small, task-local scratch files. Delete your screenshots, logs and other temporary files once they are no longer needed, such as after verifying a fix or completing the task, and clean up before ending the session. Never delete another session's files or files still in use.
- Keep all long-lived work, Cargo projects, compilation outputs, and large artifacts under `~/dev`, not `/tmp`. Use git worktrees under `~/dev` when isolated project work is needed, rather than copying projects into `/tmp`; keep Cargo target directories on disk under `~/dev` too.
- Prefer pnpm or bun over npm.
- Make invariant violations obvious. Do not silently fall back from an impossible state; justify and log any necessary fallback.
- Never rotate credentials or secrets solely because an agent read or displayed them; sessions are private.
- When you need to wait for something wait on it directly instead of sleeping. for example on the a pid to finish instead of sleep and rechecking in between.

## Verification

- Review first. Documentation, comments, and obvious nonbehavioral cleanup usually need only review and formatting of touched code. Substantial code changes usually need one final scoped compilation check; behavior-sensitive changes need focused tests.
- Finish a coherent change before running its checks. Check early only to resolve a concrete blocker. Each build, lint, test, or rendered check must answer a distinct question; do not automatically stack them or treat Clippy as cheap.
- Reuse passing results through cleanup, review, commit, and landing. Rerun only the checks invalidated by later source, dependency, configuration, or environment changes. A new workflow phase is not a reason to repeat verification.
- Keep delegated reviews read-only. Reviewers report defects and concrete verification gaps; the parent agent owns fixes and one scoped verification batch. Do not have multiple agents compile or run the same checks independently.
- Before retrying a stalled or timed-out build, inspect compiler activity, cache health, and lock contention. Avoid competing Cargo jobs against the same target directory, and fit total build parallelism to available RAM. Increase timeouts only when the process is making progress; diagnose persistent failures before rerunning a broad suite.
- Report checks actually performed, reused results, and remaining verification gaps briefly. Do not imply that review alone compiled or tested the code.
