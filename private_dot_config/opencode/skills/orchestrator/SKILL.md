---
name: orchestrator
description: Use when the user asks to orchestrate delegated work or when coordinating multiple agents, including luna, sol, or astra.
---

# Orchestrator

Coordinate delegated work for the current task. Follow the task tool's delegation
policy for worker availability, tier selection, and explicit user choices.

An explicit orchestration request makes delegated execution the default for this
task. Use workers for exploration of current behavior and limitations, research,
implementation, and review without first doing all their discovery yourself. The
parent thinks independently and owns shared decisions and integration. Targeted
reads, small edits, and unblockers remain appropriate. Loading this skill on your
own does not impose the same delegation emphasis on an ordinary task.

## Decide whether to delegate

Estimate whether independent progress or scrutiny is worth briefing, startup,
context discovery, and integration. Delegate design when its context transfers
economically; retain tightly coupled work when handing it off costs more than doing
it. No formal cost calculation or target agent count is needed.

Within the task tool's tier defaults, prefer a worker likely to finish well without
extensive correction. Escalate when complexity, uncertainty, risk, or a failed
attempt warrants it rather than repeatedly retrying an unsuitable tier. A fresh
perspective can help even on a small uncertain decision; give independent reviewers
the facts and question without requiring agreement with your conclusion.

## Dispatch work

Identify dependencies and settle necessary contracts before dependent implementation;
independent exploration need not wait for the overall design. Parallelize genuinely
independent scopes, not merely separate files. Balance expected duration, start long
prerequisites early, split oversized tasks, and batch small related ones. Keep useful
local work when available; waiting for a prerequisite is better than duplicating it.

Assign non-overlapping write ownership. Before reassignment, establish that the
previous writer stopped and inspect partial changes; obtain a handoff when available.
Intentional comparison and verification may overlap, but avoid accidentally solving
the same assignment alongside a worker.

Scale the brief to the assignment: goal, scope, editing permission, relevant context,
and acceptance criteria. Subagents do not inherit the parent conversation. Supply
known paths, decisions, and unknowns instead of requesting repeated discovery; broad
exploration is appropriate for genuinely unknown territory. Reuse relevant agent
context for follow-up; start fresh for a different scope or independent perspective.

## Stay informed

Ask for concise results with relevant evidence, changed files, checks, and unresolved
work. Implementation handoffs should explain how it works, why the approach fits,
and important tradeoffs or limitations, with useful code references. Request technical
rationale, not private reasoning or raw transcripts.

Maintain a working understanding of the evolving implementation. Inspect key code
paths and interfaces, question insufficient evidence, and resolve consequential
design changes before dependent work builds on them. Scale scrutiny to complexity
and risk rather than rereading every file. The parent should be able to explain and
defend the resulting approach, not merely cite worker approval.

Use milestones when intermediate results could change the next step; let routine
assignments finish in one pass. Prefer completion notifications and use supported
status checks when needed for a decision. Avoid repeated polling, artificial sleeps,
and invented communication channels.

Preserve each task ID, owner, scope, dependencies, status, and handoff through
compaction. Use results to update the plan and unblock remaining work.

Report meaningful progress, decisions, and blockers. Verify against acceptance
criteria; reuse valid checks unless changes, uncertainty, or risk warrant a repeat.
Finish with changes, verification, and unresolved work, distinguishing worker reports
from checks you observed.
