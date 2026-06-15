# Reason For Investigation

Beryl needed to know whether the OpenAI Codex agent runtime can be used directly as Rust crates instead of through Codex App Server, while preserving agentic behavior and gaining better control over parallel work, thread and turn data, and generated-image payload loading.

# Outcome

Codex does expose usable Rust crate surfaces for direct embedding. The strongest entry point is `codex-core-api`, which is explicitly a public facade over `codex-core` thread management APIs. The `thread-manager-sample` crate demonstrates constructing a `ThreadManager`, starting a `CodexThread`, submitting user input, and draining `EventMsg` values without running app-server.

Direct embedding is feasible, but it is not a drop-in CAS replacement. Beryl would need a new backend boundary that replaces app-server request routing, thread lifecycle operations, event projection, approval and user-input plumbing, dynamic tool reverse calls, stop handles, config/auth/environment setup, MCP/plugins/skills wiring, and compatibility policy.

There is also an intermediate option: `codex-app-server` and `codex-app-server-client` provide an in-process app-server host/client. That can remove the external process and socket boundary while preserving app-server semantics, but it keeps the app-server protocol and does not solve the current coarse history and generated-image payload behavior by itself.

The main Beryl pain points are not fully solved by existing crate APIs. `codex-thread-store` already defines `list_turns` and `list_items` shapes, including `StoredTurnItemsView` and `StoredThreadItem`, but the local store defaults these methods to unsupported. The app-server `thread/turns/list` processor still rebuilds turns by loading full rollout history on each request, and `thread/turns/items/list` returns method-not-found in the inspected source.

SQLite is suitable for forward and backward keyset cursor walking, and Codex already uses SQLite keyset pagination for thread metadata lists. The current state database schema, however, indexes `threads` and related metadata, not durable `thread_turns` or `thread_items` projections. Adding lazy item walking therefore means creating and maintaining a new projection/index from rollout replay, not just adding SQL over existing rows.

Generated images are saved to disk by core, but the canonical protocol and item model still carry the base64 `result` string in image generation events and persisted items. The safest augmentation would be a payload-light UI/history projection that exposes `saved_path` and bounded metadata without materializing or transmitting the base64 result, rather than mutating canonical history needed for replay or model context without further design review.

The likely augmentation path is:

1. Add indexed local turn and item metadata for `LocalThreadStore`, including rollback and compaction invalidation semantics.
2. Implement `ThreadStore::list_turns` and `ThreadStore::list_items` for the local store.
3. Expose those granular APIs through `codex-core-api` or a Beryl-facing crate facade.
4. Add redacted or lazy media descriptors for generated images, preserving `saved_path` and avoiding base64 payload transfer for UI/history reads.
5. Keep canonical agent replay semantics separate from UI projection semantics.

This investigation supports a Beryl V2 backend-boundary design discussion. It does not change the current project design, which still names Codex App Server as the backend authority.

# Sources

- Canonical source repository: https://github.com/openai/codex.git.
- Resolved local commit: 495da4564337099f064b25f7e2f00436f56bc076.
- Source files inspected: `codex-rs/Cargo.toml`, `codex-rs/core-api/src/lib.rs`, `codex-rs/core/src/lib.rs`, `codex-rs/core/src/thread_manager.rs`, `codex-rs/core/src/codex_thread.rs`, `codex-rs/thread-manager-sample/README.md`, `codex-rs/thread-manager-sample/src/main.rs`, `codex-rs/thread-store/src/lib.rs`, `codex-rs/thread-store/src/store.rs`, `codex-rs/thread-store/src/types.rs`, `codex-rs/thread-store/src/local/mod.rs`, `codex-rs/thread-store/src/local/read_thread.rs`, `codex-rs/app-server/src/in_process.rs`, `codex-rs/app-server-client/src/lib.rs`, `codex-rs/app-server-protocol/src/protocol/v2/thread.rs`, `codex-rs/app-server/src/request_processors/thread_processor.rs`, `codex-rs/app-server-protocol/src/protocol/thread_history.rs`, `codex-rs/protocol/src/models.rs`, `codex-rs/protocol/src/items.rs`, `codex-rs/protocol/src/protocol.rs`, `codex-rs/core/src/stream_events_utils.rs`, and `codex-rs/ext/image-generation/src/tool.rs`.
- Local Beryl sources consulted for integration impact: `doc/design.md`, `doc/features/conversation-threads/design.md`, `doc/features/transcript/design.md`, `doc/new-transcript-renderer.md`, `doc/memory/topic/codex-app-server/transcript-history-itemsview-0.137.md`, `doc/failures/cas-turn-list-latency.md`, `doc/failures/image-memory.md`, `crates/beryl-backend/src/command.rs`, `crates/beryl-backend/src/server.rs`, `crates/beryl-backend/src/session.rs`, `crates/beryl-backend/src/turn.rs`, `crates/beryl-backend/src/response_sanitizer.rs`, `crates/beryl-app/src/shell/transcript_history.rs`, and `crates/beryl-app/src/shell/transcript_media/load.rs`.
- Commands used included `rg --files`, targeted `rg -n` symbol searches, `git rev-parse HEAD`, `git remote get-url origin`, and targeted `Get-Content` reads.
