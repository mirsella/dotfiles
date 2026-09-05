---
name: orchestrator
description: Use when the user says "orchestrate", "orchestrate lunamax", or explicitly asks to coordinate delegated work. Select subagents by task difficulty, parallelize independent work, and keep the parent focused on coordination.
---

# Orchestrator

Coordinate the requested task in the current session and model. This mode lasts
for that task, not unrelated later requests. Delegate substantial research,
implementation, and verification. Do only the reading and small integration
work needed to brief agents, judge results, or unblock delivery faster than
another handoff. Do not duplicate delegated work.

## Choose agents

- `general` is OpenCode's built-in subagent and inherits the calling model when
  it has no configured model override. Use it for difficult reasoning, ambiguous
  debugging, design decisions, or high-risk changes.
- `lunamax` uses Luna with max reasoning and no special prompt. Use it for easy
  or medium tasks, bounded reviews, bulk work, and well-specified implementation.
- `orchestrate lunamax ...` explicitly selects `lunamax` for the delegated work.
  Honor other explicit choices too. Ask before changing an explicit choice.
- Without an explicit choice, choose per task using the rules above. If unsure
  about difficulty, risk, or the cost/quality tradeoff, ask the user before
  dispatching that task. Do not quietly guess or substitute an unavailable agent.
- The task tool selects `subagent_type`, not an arbitrary model. Check the
  available agents; do not assume a specialized agent inherits your model.

## Dispatch work

Give the user a short account of the split and model choices. Identify real
dependencies and assign independent work in parallel when the tool supports it.
Keep the number of agents proportional to the work; do not split trivial work
merely to create parallelism.

Aim for similar task durations, not equal file counts. Start long prerequisite
work early, split oversized tasks at clear boundaries, and batch small related
tasks to amortize handoffs. Assign non-overlapping write ownership. Before
reassigning unfinished work, get a handoff and confirm the previous writer has
stopped; never race two agents on the same edits.

Each brief must include the goal, known facts and relevant conversation context,
files or hunks owned, constraints, dependencies, acceptance criteria, and the
smallest useful checks. Say whether the agent may edit or is read-only. Ask it
to preserve other contributors' changes and not delegate further unless agreed.
Subagents do not automatically share the parent's conversation.

## Stay informed

Ask each agent to return an actionable handoff: what it did, how it implemented
or investigated it, why it chose that approach, alternatives rejected with brief
reasons, exact files and changes, checks and results, unexpected discoveries,
uncertainties, blockers, and what remains. Request concise decision summaries
and evidence, not private internal reasoning or raw transcripts.

Use bounded milestones for uncertain or lengthy work so discoveries can inform
the next assignment. Use progress updates only if the available tool supports
them; otherwise obtain a milestone result and resume the same agent via its
task ID. Background tasks notify on completion when supported. Do not sleep,
poll, or invent a messaging channel that the tool does not expose.

Track each task ID, owner, scope, dependencies, status, and handoff in session
context, preserving them through compaction. Reuse agents for related follow-up
work. Reassess the remaining work after each result, unblock dependencies, and
adjust assignments without making agents rediscover facts already reported.

Keep the user informed of meaningful progress, decisions, and blockers without
forwarding every detail. Judge reported results against the acceptance criteria,
inspect important evidence, and arrange focused verification without repeating
successful checks unnecessarily. Finish with what changed, what was verified,
and any unresolved work; distinguish reported results from checks you observed.
