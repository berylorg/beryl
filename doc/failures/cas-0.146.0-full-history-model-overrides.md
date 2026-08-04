# Scope

Codex App Server 0.146.0 native subagent model and reasoning selection.

# Invalidated Approach

Beryl's target contract required CAS to reject explicit `model` or `reasoning_effort` inputs when
`spawn_agent` uses an omitted or `"all"` full-history fork, so the child necessarily inherits the
parent's effective profile.

# Decisive Evidence

At pinned commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`, the
[native spawn handler](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs)
rejects only a full-history `agent_type` override and then applies requested model and reasoning
overrides without fork-mode awareness. The
[shared override helper](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_common.rs)
validates and mutates the child configuration, which the
[fork creation path](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/agent/control/spawn.rs)
then consumes. Model-facing guidance says full-history forks inherit, but exact 0.146.0 does not
enforce rejection.

# Why It Failed

The proposed guarantee conflated model-facing orchestration guidance with Beryl product authority.
`fork_turns` chooses the history used to seed a spawned subagent's child thread; `model` and
`reasoning_effort` choose that child's execution profile. No Beryl ownership, identity, durability,
or lifecycle requirement couples those independent choices.

# Course Correction

The Operator rejected the same-profile requirement. Beryl accepts CAS 0.146.0's precedence: each
explicit value precedes its configured subagent default; neither resolved retains the parent
profile; reasoning alone applies to the parent model; and a selected model without reasoning uses
that model's catalog default. Profile selection remains independent of fresh, bounded-history, or
full-history child context. Do not reintroduce a same-model requirement from an agent tool's usage
guidance or a particular orchestration workflow.

# Affected Authority And Risk

The correction affects `doc/design.md`, `doc/systems/cas-live-syndic-transcript/design.md`, Phase 84
of `doc/plan.md`, and the 0.146.0 investigation memory. A particular live orchestration tool may
still restrict which argument combinations it accepts; that surface limitation does not define
Beryl's CAS compatibility architecture.
