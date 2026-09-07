---
name: orchestrator
description: Use when the user asks to orchestrate or coordinate delegated work, including with luna, sol, or astra.
---

# Orchestrator

Coordinate the requested task in the current session; this mode ends with that
task. Agent choice follows the model-specific policy injected by the orchestrator
plugin. Delegate substantial research, implementation, and verification. Keep
only briefing, integration, judgment, and small unblockers in the parent. Do not
duplicate delegated work.

## Dispatch work

Tell the user the split and model choices. Identify dependencies, parallelize
independent work, and keep agent count proportional to the task. Balance expected
duration rather than file count; start long prerequisites early, split oversized
tasks at clear boundaries, and batch small related tasks. Assign non-overlapping
write ownership. Before reassigning unfinished work, obtain a handoff and confirm
the prior writer stopped.

Brief each agent with the goal, relevant facts and conversation context, owned
scope, constraints, dependencies, acceptance criteria, smallest useful checks,
and whether it may edit. State that subagents lack the parent conversation, must
preserve others' changes, and must not delegate further unless agreed.

## Stay informed

Require a concise handoff covering the outcome, approach and rationale, rejected
alternatives, exact changes, checks and evidence, discoveries, uncertainties,
blockers, and remaining work. Do not request private reasoning or raw transcripts.

For lengthy or uncertain work, use bounded milestones and resume the same task ID.
Use progress updates only when supported; otherwise wait for the milestone result.
Do not sleep, poll, or invent communication channels.

Track each task ID, owner, scope, dependencies, status, and handoff through
compaction. Reuse agents for related follow-up. After each result, reassess and
unblock remaining work without making agents rediscover known facts.

Report meaningful progress, decisions, and blockers without forwarding every
detail. Judge results against acceptance criteria, inspect important evidence,
and arrange focused verification without repeating valid checks. Finish with
changes, verification, and unresolved work; distinguish reports from checks you
observed.
