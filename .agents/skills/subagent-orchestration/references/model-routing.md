# Model and Reasoning Routing

Use this reference only for current model names, supported reasoning values, and spawn mechanics. The parent skill owns selection, escalation, authorization, ownership, review, and effort-control policy.

## Orchestrator

Recommended pre-session configuration: `gpt-6-astra` with `medium` reasoning for the persistent main orchestrator. A loaded skill cannot change an already-started main profile.

## Routing Maps

- Balanced: `gpt-5.6-terra`; frontier: `gpt-6-astra`.
- Shallow: `low`; normal: `medium`; deep: `high`; critical: `xhigh`.
- Quality-First, only with explicit current-task Operator authorization: `gpt-6-astra` / `max`.

Do not use Sol or `ultra`. Both mapped models support the ordinary efforts above. Combine capability and effort independently according to the parent skill; do not raise one merely because the other changes.

## Spawn Mechanics

Set `model` and `reasoning_effort` explicitly for every routed subagent. Only the root may spawn. Use `fork_turns="none"` by default so the task packet supplies complete context. Use a bounded positive string such as `"3"` only when recent conversational context is genuinely required; do not combine a full-history fork with a model or reasoning override.
