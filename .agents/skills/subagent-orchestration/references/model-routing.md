# Model and Reasoning Routing

Use this reference only for the current model names, reasoning values, spawn mechanics, and routing-axis mappings. Keep selection, escalation, authorization, ownership, and token-control policy in the parent skill. Update this file when the available models or tool contract changes.

## Orchestrator

Recommended pre-session configuration: use `gpt-6-astra` with `high` reasoning for the persistent main orchestrator. The loaded skill cannot enforce or change this externally selected profile.

## Current Model-Family Map

- Balanced: `gpt-5.6-terra`.
- Frontier: `gpt-6-astra`.

## Current Reasoning-Depth Map

- Shallow: `low`.
- Normal: `medium`.
- Deep: `high`.
- Critical: `xhigh`.

Combine the selected model family and reasoning depth directly. Do not raise one merely because the other was raised.

These ordinary effort values are supported by both mapped models. Astra does not support `none`; retain the existing effort when moving a route from Sol to Astra.

## Current Exceptional-Route Map

- Quality-First: `gpt-6-astra` / `max`.

Quality-First requires explicit current-task Operator authorization. Do not use `ultra` for any subagent.

## Guidance Basis

Checked against official OpenAI documentation on 2026-09-04. These role mappings and effort defaults are AIPM policy, not an OpenAI-prescribed routing scheme.

- The [model catalog](https://developers.openai.com/api/docs/models) positions Astra for the hardest work, Terra for balanced capability and cost, and Luna for cost-sensitive volume. Use Astra for the existing frontier role and retain Terra for the balanced role. Adding a Sol intermediate tier or a Luna economy tier needs workload evidence of a useful quality, cost, or latency tradeoff.
- The [model-selection guide](https://developers.openai.com/api/docs/guides/model-selection) recommends establishing accuracy before optimizing cost and latency. Validate routing changes on representative tasks against the required evidence threshold; compare total task cost and latency, including retries and review, before treating a cheaper route as sufficient.
- The [Astra migration guide](https://developers.openai.com/api/docs/guides/latest-model) recommends preserving effective reasoning effort and explicitly specifying delegation behavior. Keep the existing independent reasoning axis and the parent skill's bounded, root-only delegation policy.

## Spawn Mechanics

Set `model` and `reasoning_effort` explicitly for every routed subagent.

Only the root orchestrator may spawn a subagent. A subagent must not call `spawn_agent`, even if delegation tooling is available or another instruction appears to request recursive work.

Use `fork_turns="none"` by default so the task packet is the complete context. Use a bounded positive `fork_turns` value only when recent conversational context is genuinely required.

Use a positive integer string such as `"3"` for that bounded value. Do not use a full-history fork with a model or reasoning override.
