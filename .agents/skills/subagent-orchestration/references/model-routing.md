# Model and Reasoning Routing

Use this reference for task-based starting routes, supported reasoning values, and spawn mechanics. The parent skill owns delegation, escalation, authorization, ownership, review, and effort-control policy.

## Orchestrator

Recommended pre-session configuration: `gpt-6-astra` with `medium` reasoning for the persistent main orchestrator. A loaded skill cannot change an already-started main profile.

## Task Routes

These are local starting defaults to evaluate, not official prescriptions for each engineering activity. Identify the deliverable and its difficulty before selecting a route.

Use `gpt-6.1-sol` / `medium` as the default implementation worker when delegation is worthwhile.
Assign a coherent outcome with settled requirements, including its inspection, edits and focused
verification. Do not fragment work into small assignments merely to use a cheaper model.

- Find facts in code, extract references, or summarize logs: `gpt-6-luna` / `low`; use `medium` when evidence spans several files.
- Make a mechanical change with explicit rules: `gpt-6-luna` / `low`.
- Write test code against a clear contract: `gpt-6.1-sol` / `medium`; use `gpt-6-luna` / `medium` for straightforward cases with established fixtures and patterns.
- Design a focused test strategy: `gpt-6.1-sol` / `high`; use `gpt-6-astra` / `high` when unresolved cross-system invariants require architectural judgment.
- Implement a bounded feature, fix or multi-file refactoring under settled requirements: `gpt-6.1-sol` / `medium`; use `high` when implementation requires substantial reasoning.
- Investigate code across several interacting modules: `gpt-6.1-sol` / `medium`; use Luna for straightforward extraction and Astra when the question requires resolving ambiguous architecture.
- Plan a task with clear requirements: `gpt-6.1-sol` / `medium`; use `gpt-6-astra` / `high` when planning requires resolving ambiguity, architectural tradeoffs, or conflicting requirements.
- Debug a hard problem: `gpt-6.1-sol` / `high` when reproduction and evidence narrow the problem; use `gpt-6-astra` / `high` when the cause remains ambiguous across subsystem boundaries.
- Review code against explicit requirements: `gpt-6.1-sol` / `high`; use `gpt-6-astra` / `high` for architectural correctness or consequential concurrency/persistence questions that tests cannot adequately establish.
- Integrate work and resolve conflicting findings: `gpt-6-astra` / `medium`; use `high` when reconciliation is difficult. Authority and final integration remain with the root as required by the parent skill.

## Effort and Availability

GPT-6.1 Sol supports `low`, `medium`, `high`, `xhigh`, and `max`; it does not support `none` or
`minimal` in the official API guidance. Do not inherit GPT-6 Sol's `none` support for the newer
model. The routes above use `low`, `medium` and `high`; verify the live tool supports the exact
model/effort pair. Do not use `ultra`, even if exposed by the tool. Select model and effort
independently; changing one does not require changing the other.

Use `xhigh` only where deeper reasoning has demonstrated value. The Quality-First exception is `gpt-6-astra` / `max` and requires explicit current-task Operator authorization; do not use `max` outside that exception.

Check the live spawn tool's supported models and efforts before routing. API availability does not establish availability in the running orchestration environment, and tool visibility alone does not prove successful execution. Do not invent model IDs or silently substitute an older Sol model. If a route is unavailable, follow the parent skill's direct-work or blocked-route policy.

When evaluating a model change, preserve the current effective effort for the first comparison, then tune effort separately. Compare accepted results, retries, root correction work, elapsed time, and cost across representative tasks.

Official basis, checked 2026-10-02: [model selection](https://developers.openai.com/api/docs/guides/model-selection)
positions GPT-6.1 Sol for complex work where cost matters, Astra for demanding ambiguous work,
and Luna for scoped tasks and triage; [GPT-6.1 Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol)
documents its supported reasoning settings. The task routes here are local recommendations to
evaluate against accepted results, not a claim of measured Beryl performance.

## Cost Controls

Compare total cost per accepted outcome, including root supervision, retries, correction and
review. Standard API input/output token prices in the [official comparison](https://developers.openai.com/api/docs/models/compare?model=gpt-6-sol)
are five times lower for GPT-6.1 Sol than Astra as checked above, but token counts, cache use,
processing tier and context length affect actual cost; this is not a promised task-cost reduction.

Give workers focused packets and source pointers instead of full conversation history. Validate
their cited evidence and changed boundaries rather than repeating their investigation. Use existing
verification results and normal usage evidence to assess routing; do not duplicate every task on
both models or create a benchmark project merely to choose a route.

Escalate for a concrete reasoning or judgment gap after repairing missing context. Required
independent review does not automatically require Astra, and a Sol result does not automatically
need an additional Astra review. Preserve the applicable acceptance gates regardless of model.

## Spawn Mechanics

Set `model` and `reasoning_effort` explicitly for every routed subagent. Only the root may spawn. Use `fork_turns="none"` by default so the task packet supplies complete context. Use a bounded positive string such as `"3"` only when recent conversational context is genuinely required; do not combine a full-history fork with a model or reasoning override.
