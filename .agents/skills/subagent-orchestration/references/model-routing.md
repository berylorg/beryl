# Model and Reasoning Routing

Use this reference only for the current model names, reasoning values, spawn mechanics, and routing-axis mappings. Keep selection, escalation, authorization, ownership, and token-control policy in the parent skill. Update this file when the available models or tool contract changes.

## Orchestrator

Recommended pre-session configuration: use `gpt-5.6-sol` with `high` reasoning for the persistent main orchestrator. The loaded skill cannot enforce or change this externally selected profile.

## Current Model-Family Map

- Balanced: `gpt-5.6-terra`.
- Frontier: `gpt-5.6-sol`.

## Current Reasoning-Depth Map

- Shallow: `low`.
- Normal: `medium`.
- Deep: `high`.
- Critical: `xhigh`.

Combine the selected model family and reasoning depth directly. Do not raise one merely because the other was raised.

## Current Exceptional-Route Map

- Quality-First: `gpt-5.6-sol` / `max`.

Quality-First requires explicit current-task Operator authorization. Do not use `ultra` for any subagent.

## Spawn Mechanics

Set `model` and `reasoning_effort` explicitly for every routed subagent.

Only the root orchestrator may spawn a subagent. A subagent must not call `spawn_agent`, even if delegation tooling is available or another instruction appears to request recursive work.

Use `fork_turns="none"` by default so the task packet is the complete context. Use a bounded positive `fork_turns` value only when recent conversational context is genuinely required.

Use a positive integer string such as `"3"` for that bounded value. Do not use a full-history fork with a model or reasoning override.
