# Reason For Investigation

The August 2026 repeated-yield acceptance trace appeared to lose Beryl subagent activity. This investigation identifies the exact `codex-cli 0.146.0` app-server multi-agent v2 protocol behavior needed to distinguish a lost client correlation from missing backend activity.

# Outcome

At exact CAS 0.146.0, the multi-agent v2 `spawn`, `send`, `followup`, and `interrupt` paths emit only a completed `subAgentActivity` item after success. The item carries `id`, `kind`, `agentThreadId`, and `agentPath`; `kind` is `started`, `interacted`, or `interrupted`. Failed operations emit no such activity. `wait_agent` remains a legacy `collabAgentToolCall` started/completed flow, and v1 remains legacy.

The app-server v2 event mapping turns those core events into `ThreadItem::SubAgentActivity` only as completed items. Beryl must correlate v2 activity by `agentThreadId`, keep `thread/read` nickname resolution, and never render `agentPath` as a nickname. It must not infer model or reasoning effort, and an `interacted` event cannot distinguish `send` from `followup`. Legacy collaboration activity remains supported alongside v2.

# Sources

- Installed target: `codex-cli 0.146.0`; executable SHA-256 `D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`.
- Exact-target identity and schema-bundle evidence: canonical upstream repository https://github.com/openai/codex.git; requested ref `rust-v0.146.0` resolved to `be449751a978f02e5bbba886999662956c7f38f5`. The release commit object was unavailable locally, so this note does not claim source inspection at that commit.
- Official source-level shape: [multi-agent: add path-based v2 activity tracking (#27007)](https://github.com/openai/codex/commit/fae270932065355b5d7f197b3f1c72912588369b), commit `fae270932065355b5d7f197b3f1c72912588369b`, and current official Codex source. Inspected paths and symbols: `codex-rs/protocol/src/protocol.rs` (`SubAgentActivityEvent`, `SubAgentActivityKind`); `codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs`, `message_tool.rs`, `interrupt_agent.rs`, and `wait.rs`; `codex-rs/app-server-protocol/src/protocol/v2/item.rs` (`ThreadItem::SubAgentActivity`); and `codex-rs/app-server-protocol/src/event_mapping.rs` (completed-only mapping).
