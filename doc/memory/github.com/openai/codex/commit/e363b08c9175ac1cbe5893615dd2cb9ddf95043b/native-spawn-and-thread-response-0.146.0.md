# Reason For Investigation

Phase 85 needs exact 0.146.0 evidence for additive thread response members and native
`spawn_agent` model/reasoning selection. This note records source facts only; it does not make
design or implementation decisions.

# Outcome

## Thread serialization and schema gating

At the pinned source instance, `Thread` in
`codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs` derives `Serialize` with
`rename_all = "camelCase"`; its declaration order is its serializer member order:

1. `id`
2. `extra` (experimental)
3. `sessionId`
4. `forkedFromId`
5. `parentThreadId`
6. `preview`
7. `ephemeral`
8. `isPinned`
9. `historyMode` (experimental)
10. `modelProvider`
11. `createdAt`
12. `updatedAt`
13. `recencyAt`
14. `status`
15. `path`
16. `cwd`
17. `cliVersion`
18. `source`
19. `canAcceptDirectInput` (experimental)
20. `threadSource`
21. `agentNickname`
22. `agentRole`
23. `gitInfo`
24. `name`
25. `turns`

`is_pinned` is a `bool` with `#[serde(default)]` and no `#[experimental(...)]` marker;
deserialization accepts an omitted value as false, while serialization does not skip the member.
`can_accept_direct_input` is `Option<bool>` and is marked
`#[experimental("thread.canAcceptDirectInput")]`. Local canonical 0.146.0 schema regeneration
corroborates that stable output includes `Thread.isPinned`, whereas `canAcceptDirectInput` occurs
only with `generate-json-schema --experimental`.

`ThreadResumeResponse` in `thread.rs` has this exact final declared/serialized tail:
`reasoningEffort`, `multiAgentMode` (experimental), `initialTurnsPage` (experimental),
`turnsBackwardsCursor` (experimental), then `itemsBackwardsCursor` (experimental). The two cursor
members are `Option<String>` with `#[serde(default)]`; both are gated from stable generated schema
and appear only in experimental generated schema.

## Native `spawn_agent` exposure and profile resolution

For MultiAgentV2, `spawn_agent_common_properties_v2` creates optional `model` and
`reasoning_effort` schema properties. `create_spawn_agent_tool_v2` removes exactly those two only
when `SpawnAgentToolOptions.expose_spawn_agent_model_overrides` is false. `add_collaboration_tools`
passes that option from the effective
`turn_context.config.multi_agent_v2.expose_spawn_agent_model_overrides`; the exact default
`MultiAgentV2Config` sets it to true. Thus the model-facing native tool schema exposes the two
optional inputs when that effective setting is true, including when unrelated spawn metadata is
hidden.

The handler's `SpawnAgentArgs` accepts `model: Option<String>` and
`reasoning_effort: Option<ReasoningEffort>`. `build_agent_spawn_config` starts with the parent
turn's effective model and effective/default reasoning effort. The exact resolution helper first
uses each explicit input in preference to its configured subagent default, then has four branches:

1. If neither effective value resolves, it returns without changing that inherited parent profile.
2. With only reasoning resolved, it validates the effort against the parent model and applies it.
3. With a model but no reasoning resolved, it selects that model and uses its catalog default
   reasoning level.
4. With both resolved, it selects the model and validates/applies the effort against that selected
   model.

Fork history does not alter those branches. `handle_spawn_agent` parses `fork_turns`, rejects an
`agent_type` override only for full history, then calls
`apply_requested_spawn_agent_model_overrides` before its fork-only role condition and before
handing the resulting config plus fork mode to `AgentControl`. Therefore fresh (`none`), bounded
(`N`), and full-history (`all` or omitted) contexts flow independently of this profile-resolution
code; the source does not reject model/reasoning overrides solely because the fork is full history.

# Sources

Canonical remote: `https://github.com/openai/codex`. Requested and resolved source instance:
commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b` (`codex-cli 0.146.0`). Exact files were accessed
2026-08-03.

- [`Thread`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs#L153-L219)
  defines the member order, `is_pinned` serde default, and experimental direct-input marker.
- [`ThreadResumeResponse`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L384-L436)
  defines the response tail and experimental cursor markers.
- [`spawn_agent_common_properties_v2` and `create_spawn_agent_tool_v2`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_spec.rs#L92-L134)
  plus [the V2 property map](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_spec.rs#L590-L632)
  define exposure/removal of the two model-facing schema properties.
- [`add_collaboration_tools`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/spec_plan.rs#L769-L828)
  passes the effective MultiAgentV2 setting into the native handler's tool options.
- [`MultiAgentV2Config::defaults_for_max_concurrency` and its resolver](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/config/mod.rs#L1082-L1128)
  and [the effective setting resolution](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/config/mod.rs#L2418-L2490)
  show the true default and its configured override path.
- [`handle_spawn_agent` and `SpawnAgentArgs::fork_mode`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs#L35-L120)
  and [its input fields/fork parser](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs#L165-L208)
  show input acceptance, full-fork ordering, and the config/fork handoff.
- [`build_agent_spawn_config`, `build_agent_shared_config`, and
  `apply_requested_spawn_agent_model_overrides`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_common.rs#L157-L282)
  define inherited parent profile construction, explicit-before-default precedence, and all four
  resolution branches.
- [`spawn_agent_internal` and `spawn_forked_thread`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/agent/control/spawn.rs#L346-L456)
  and [the fork path](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/agent/control/spawn.rs#L539-L644)
  consume the already selected config separately from history creation.

Local executable corroboration, without a model turn:

- `codex.exe --version` returned `codex-cli 0.146.0`.
- `Get-FileHash -Algorithm SHA256 -LiteralPath <admitted-codex.exe>` returned
  `D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`.
- Stable schema command:
  `codex.exe app-server generate-json-schema --out <schema-output>`.
- Experimental schema command:
  `codex.exe app-server generate-json-schema --experimental --out <schema-output>`.
