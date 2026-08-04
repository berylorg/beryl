# Reason For Investigation

Beryl needs heterogeneous CAS-native subagents so the orchestrating model can select a cheaper or
more capable model and reasoning effort for each bounded task without Beryl reimplementing CAS
collaboration semantics. Codex App Server 0.144.1 exposed only task, message, and history-fork
arguments under Beryl's effective configuration. The installed 0.146.0 executable was investigated
to determine whether it removes that architectural gap.

# Outcome

The installed executable is `codex-cli 0.146.0`, SHA-256
`D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`.

CAS 0.146.0 solves the model-selection part of the problem through its native `spawn_agent` tool.
The tool accepts optional `model` and `reasoning_effort` arguments. Each explicit value precedes its
configured subagent default. With neither resolved, the child retains the parent model and effort.
A resolved effort alone is validated against the parent model. A resolved model without an effort
uses that model's catalog default, while a resolved pair is validated together. The resulting
thread is a real CAS-managed collaboration child, so the existing send, follow-up, wait, interrupt,
list, lineage, and lifecycle behavior remains native.

CAS 0.146.0's model-facing usage guidance recommends a fresh or bounded-history spawn when supplying
an override, but its implementation keeps the two choices independent. A full-history fork rejects
an `agent_type` override, then still validates and applies `model` and `reasoning_effort` before
creating the child. The resulting profile may therefore differ from the parent's regardless of
whether the child thread receives fresh, bounded, or full parent history.

The important 0.146.0 change is exposure policy rather than the underlying spawn implementation.
Version 0.144.1 already contained model and reasoning fields internally, but
`hide_spawn_agent_metadata = true` removed `agent_type`, `model`, `reasoning_effort`, and
`service_tier` together from the model-visible tool. Version 0.146.0 decouples model selection from
the unrelated metadata switch. Its new `expose_spawn_agent_model_overrides` setting defaults to
`true`, so `model` and `reasoning_effort` remain visible even while agent-type and service-tier
metadata stay hidden by default.

The exact managed configuration path is nested under the feature entry. Beryl must enable both
`features.multi_agent_v2.enabled = true` and
`features.multi_agent_v2.expose_spawn_agent_model_overrides = true`; the earlier shorthand
`multi_agent_v2.expose_spawn_agent_model_overrides` is not a valid strict-config path. Because
configured layers can supersede ordinary command-line overrides, compatibility must prove the
effective values observed by the running session rather than trust launch arguments or defaults.
Beryl supplies the pair as the single override
`features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}`. Its bounded
`config/read` proof requires both effective booleans to be true and both dotted value origins to be
the `sessionFlags` layer.

No client-callable JSON-RPC `agent/spawn` method was added. The 0.146.0 ClientRequest schema still
uses the ordinary thread and turn methods. That absence is no longer a blocker for Beryl's intended
design because the model can make the granular choice through CAS's own native spawn primitive;
Beryl does not need a dynamic tool that imitates spawning.

On 2026-08-03 both `C:\Users\user\apps\bin\codex.exe` and the adjacent
`codex-0.146.0.exe` reported 0.146.0 with the installed digest recorded above. This confirms the
available artifact, but Beryl still must launch the canonical executable stored in each admitted
runtime record; neither directory adjacency nor `PATH` is runtime-selection authority.

# Sources

- Installed 0.144.1 and 0.146.0 executables and their generated stable JSON Schema bundles,
  inspected on 2026-08-03.
- [CAS 0.146.0 native spawn implementation](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs)
- [CAS 0.146.0 shared spawn override implementation](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_common.rs)
- [CAS 0.146.0 fork creation path](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/agent/control/spawn.rs)
- [CAS 0.146.0 spawn tool schema and exposure policy](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_spec.rs)
- [CAS 0.146.0 tool-plan configuration](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/spec_plan.rs)
- [CAS 0.146.0 multi-agent configuration and override usage guidance](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/config/mod.rs)
- [CAS 0.144.1 spawn tool schema](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/tools/handlers/multi_agents_spec.rs)
- [Codex App Server API overview](https://developers.openai.com/codex/app-server#api-overview)
