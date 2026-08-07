# Reason For Investigation

Beryl needs an exact `codex-cli 0.146.0` answer for whether App Server multi-agent activity can provide a stable child display identity without a separate metadata read. The investigation distinguishes the model-visible collaboration tool from the App Server RPC protocol, identifies the origin of hierarchical strings such as `/root/foo`, and checks Beryl's existing normalization and legacy compatibility.

# Outcome

The public App Server protocol has no client-callable subagent-spawn RPC. A CAS client starts turns and observes their items and notifications; the model invokes the internal collaboration tool. Consequently, an App Server client cannot directly choose a child name, path, or nickname at spawn time.

For the exact 0.146.0 multi-agent v2 collaboration tool, the tool caller must supply `task_name` and `message`. `task_name` accepts lowercase ASCII letters, digits, and underscores. It is not a nickname. The backend derives the logical `AgentPath` by joining the task name to the parent's path, starting at `/root`; nested v2 spawns therefore form paths such as `/root/parent/child`. No request field lets the caller choose the random backend nickname or an arbitrary full path.

The v2 spawn tool result normally contains `task_name`, whose value is the canonical full `AgentPath`, and `nickname`, which may be null. When `multi_agent_v2.hide_spawn_agent_metadata` is enabled, the result deliberately contains only `task_name`. It does not return the child thread UUID. This result is tool output delivered inside the agent turn, not a dedicated App Server spawn response that a CAS client controls.

The App Server v2 activity observed by a client is different. A successful v2 spawn produces a completed `subAgentActivity` item containing `agentThreadId` and `agentPath`, but no nickname. The same item shape is used by the v2 interaction and interrupt operations. Thus the event supplies the durable backend thread correlation key and the logical collaboration path, while nickname remains separate metadata.

The installed experimental `Thread` schema exposes top-level `agentNickname`, `agentRole`, `parentThreadId`, and `source`. For a spawned child, `source.subAgent.thread_spawn` contains required `parent_thread_id` and `depth`, with optional `agent_nickname`, `agent_path`, and `agent_role`. `thread/read` accepts `threadId` plus optional `includeTurns` and returns the `Thread`; a metadata-only read remains the reliable recovery path when Beryl has only a v2 activity item and requires the authoritative nickname. A separate read is not logically required when the same client has already obtained usable Thread metadata from another response or notification, but the v2 `subAgentActivity` item itself cannot resolve a nickname.

The installed default and experimental schemas expose the same identity contract: both contain the Thread fields above, including nested spawn-source `agent_path`, and both contain `SubAgentActivityThreadItem` with required `agentPath`, `agentThreadId`, `id`, `kind`, and `type`. Both also include started and completed item notification envelopes. Relevant experimental-only Thread additions are `canAcceptDirectInput`, `extra`, and `historyMode`; they do not change subagent identity availability.

The exact 0.146.0 v1 collaboration tool has different compatibility behavior. Its spawn request has no `task_name`, passes no task component into the spawn source, and therefore cannot produce a deterministic `/root/foo` path. Its tool result contains `agent_id`, the child thread UUID, and `nickname`, with no agent path. The raw legacy spawn-end notification likewise carries the new thread id, nickname, and role rather than `agentPath`. `wait_agent` and all v1 collaboration activity continue through the legacy lifecycle shape.

Accordingly, `/root/foo` is neither a filesystem path nor merely a CLI rendering invention. It is the serialized core-protocol `AgentPath`, derived from the v2 collaboration task name and surfaced as `agentPath` in App Server `subAgentActivity`. The CLI can project that protocol value directly, which is why CLI hierarchical strings resemble collaboration task paths. It is not equivalent to the separately reserved backend nickname.

Beryl's adopted Activity-panel contract uses the exact nonblank v2 `agentPath` as the child display label and keeps `agentThreadId` as the correlation and ownership key. Activity no longer performs separate Thread metadata reads to resolve random nicknames. The graph traversal for nested activity remains keyed by backend thread ids and cycle bounded; it does not infer ownership from path strings.

`agentPath` is not a universal identity across every protocol generation, so the presentation contract keeps explicit compatibility behavior:

- The exact v2 path is a caller-chosen task identity, not the backend nickname.
- Legacy/v1 and `wait_agent` activity do not provide it.
- A v2 item with a missing or blank `agentThreadId` cannot be safely attributed to a child merely because it has a path.
- Nested paths describe the v2 task hierarchy, while Beryl's authoritative ownership and selection graph uses thread UUIDs.
- Pathless legacy activity uses the fixed `Subagent` label.
- A malformed v2 activity item whose required path is missing or blank remains visibly empty rather than being reclassified as legacy.

This is a feature-contract decision rather than a normalization shortcut: Beryl deliberately presents the collaboration task hierarchy, avoids Activity-only nickname readback complexity, and never exposes a thread UUID as a user-facing label.

# Sources

- Installed target: `C:\Users\user\apps\bin\codex.exe`; `codex.exe --version` reported `codex-cli 0.146.0`; executable SHA-256 `D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`.
- Exact schema commands: `codex.exe app-server generate-json-schema --out <task-temp>\stable` and `codex.exe app-server generate-json-schema --experimental --out <task-temp>\experimental`.
- Installed stable and experimental bundles: `codex_app_server_protocol.v2.schemas.json`, definitions `Thread`, `SessionSource`, `SubAgentSource`, `SubAgentActivityThreadItem`, `ItemStartedNotification`, and `ItemCompletedNotification`; generated files `v2/ThreadReadParams.json`, `v2/ThreadReadResponse.json`, `v2/ItemStartedNotification.json`, and `v2/ItemCompletedNotification.json`. Experimental-only identity-adjacent inspection also covered `v2/ThreadItemsListParams.json` and `v2/ThreadItemsListResponse.json`.
- Canonical upstream repository: https://github.com/openai/codex.git. Requested annotated tag `rust-v0.146.0` has tag object `be449751a978f02e5bbba886999662956c7f38f5` and peels to exact release commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`, dated 2026-07-28. Remote HEAD observed on 2026-08-06 was `57f42a81131ccf5933e7ec5dc659c381eeb5d72b`; it was not used to override release behavior.
- Exact release source: [`multi_agents_spec.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_spec.rs), especially `create_collaboration_spawn_tool`, `create_spawn_agent_tool`, and the v1/v2 output schemas; this defines v2 `task_name` and the differing v1/v2 results.
- Exact release source: [`multi_agents_common.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_common.rs), `thread_spawn_source`; this derives a child path from the parent `AgentPath` plus the v2 task name.
- Exact release source: [`multi_agents_v2/spawn.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs), `SpawnAgentArgs` and spawn result construction; this validates the v2 inputs and returns canonical task path plus optional nickname.
- Exact release source: [`multi_agents/spawn.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents/spawn.rs), v1 `SpawnAgentArgs`, `thread_spawn_source(..., None)`, and result construction; this establishes v1's child UUID/nickname result and lack of task path.
- Exact release source: [`agent_path.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/protocol/src/agent_path.rs), `AgentPath`, `ROOT`, `root`, and `join`; this defines `/root` and the logical hierarchical serialization.
- Exact release source: [`items.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/protocol/src/items.rs), `SubAgentActivityEvent`; [`event_mapping.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/event_mapping.rs), subagent activity mapping; these expose thread id and path to App Server items.
- Exact release source: [`legacy_events.rs`](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/protocol/src/legacy_events.rs), `CollabAgentSpawnEndEvent`; this exposes legacy child thread id, nickname, and role.
- Beryl protocol and normalization use sites: `crates/beryl-backend/src/protocol.rs` (`subagent_source_agent_nickname`, `ThreadSummary` deserialization), `crates/beryl-backend/src/turn.rs` (`GenericThreadItem`, `subAgentActivity` normalization, legacy spawn-end label update), and `crates/beryl-backend/src/activity.rs` (`ToolActivityEvent.sub_agent_activity_path`).
- Beryl projection use site: `crates/beryl-app/src/shell/tool_activity.rs` (`apply_tool_activity`, exact path labels, child ownership, lifecycle ordering, and nested ancestry traversal).
- Beryl authority and verification fixtures: `doc/features/activity-panel/design.md`, `doc/app-server-contract.md`, `crates/beryl-backend/doc/design.md`, `crates/beryl-app/doc/design.md`, `crates/beryl-backend/tests/turn_protocol.rs`, and `crates/beryl-app/tests/tool_activity.rs`.
- Access date for official repository sources: 2026-08-06. Only official OpenAI repository source, installed generated schema, and workspace primary sources were used.
