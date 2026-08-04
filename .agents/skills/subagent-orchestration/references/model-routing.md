# Model and Reasoning Routing

Use this reference only for the current model names, reasoning values, spawn mechanics, and routing-class mappings. Keep selection, escalation, authorization, ownership, and token-control policy in the parent skill. Update this file when the available models or tool contract changes.

## Orchestrator

Recommended pre-session configuration: use `gpt-5.6-sol` with `high` reasoning for the persistent main orchestrator. The loaded skill cannot enforce or change this externally selected profile.

## Current Routing-Class Map

- Economy: `gpt-5.6-terra` / `low`.
- Standard: `gpt-5.6-terra` / `medium`.
- Careful: `gpt-5.6-terra` / `high`.
- Judgment: `gpt-5.6-sol` / `medium`.
- Deep: `gpt-5.6-sol` / `high`.
- Critical: `gpt-5.6-sol` / `xhigh`.

Do not create a standard `gpt-5.6-terra` / `xhigh` class.

## Current Exceptional-Route Map

- Quality-First: `gpt-5.6-sol` / `max`.
- Nested: `gpt-5.6-sol` / `ultra`.

## Spawn Mechanics

Set `model` and `reasoning_effort` explicitly for every routed subagent.

Use `fork_turns="none"` by default so the task packet is the complete context. Use a bounded positive `fork_turns` value only when recent conversational context is genuinely required.

Use a positive integer string such as `"3"` for that bounded value. `fork_turns` selects child
context, not the desired child model or reasoning profile. A full-history child may use an explicit
profile when the active spawn tool accepts that argument combination; a narrower live tool contract
is a surface constraint, not a project architecture rule.
