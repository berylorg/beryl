# Reason For Investigation

Beryl's planned bounded history reconciliation could not recover exact compaction identity and actual terminal outcome after missing client notifications. Determine a viable evidence source without loading full transcript history or inferring success from idle.

# Outcome

CAS generates the manual compaction submission id before enqueue; Core uses it as the compaction turn id. Compact-start currently discards that id. A prepared-compaction handle can reuse the existing Core submission primitive while allowing CAS to register an exact receipt before enqueue. Registering after submission returns would race immediate completion.

CAS's thread listener consumes raw Core events before sending client notifications. A bounded process-local registry at that boundary can retain operation outcome independently of a disconnected client. Exact client-generated operation identity is necessary when acknowledgement is lost: another manual compaction replaces active tasks, so selecting a newer compaction from history is unsafe.

Successful compaction requires both a completed `ContextCompaction` item and error-free exact `TurnComplete`. `CompactTask::run` swallows some errors, including errors occurring before the compaction item starts; a no-error terminal alone can therefore falsely imply compaction success. Exact `TurnAborted` captures pre-hook, post-hook, and replacement interruption. A listener generation gap or terminal without sufficient evidence must remain unknown.

History is not a substitute: summary projection removes compaction items, inactive in-progress turns may become synthetic interrupted turns, and failed attempts before item creation have no compaction-specific history marker. Receipt retention does not require a storage migration. Its lifetime is limited to one CAS process and reliable listener generation; client disconnect alone does not invalidate it.

The status-line design owns the selected receipt contract and bounded lifetime. This note records source evidence only.

# Sources

- Repository: `https://github.com/berylorg/codex-fork`, origin `git@github.com:berylorg/codex-fork.git`; HEAD `51eef61b78bb1a054f181e15690849194d73dc71`, inspected 2026-09-05. Investigation covered only `codex-rs/**`. The working tree includes existing Beryl-owned changes to Core task/context APIs and app-server protocol/initialization/schema; findings describe that working tree, not an unmodified commit.
- `codex-rs/app-server/src/request_processors/thread_processor.rs`: `thread_compact_start_inner`, `load_thread`, `apply_thread_turns_items_view`, and `normalize_thread_turns_status` establish submission, listener setup limitations, and history projection limits.
- `codex-rs/core/src/session/mod.rs`: `SessionIo::submit_with_trace`, `submit_with_id`, and terminal-error capture establish generated identity, enqueue primitive, and error provenance.
- `codex-rs/core/src/session/handlers.rs`: compact handling uses submission identity for the manual compaction turn.
- `codex-rs/core/src/tasks/mod.rs`: task replacement, exact `TurnAborted`, and terminal `TurnComplete` generation.
- `codex-rs/core/src/tasks/compact.rs`, `core/src/compact.rs`, `core/src/compact_remote.rs`, and `core/src/compact_remote_v2.rs`: swallowed non-abort errors, hook timing, completed compaction marker, and early step-capture failures.
- `codex-rs/app-server/src/request_processors/thread_lifecycle.rs`: raw event handling before outgoing notification translation provides the receipt observation boundary.

# Refresh Triggers

Recheck after changes to submission ids, compact task terminal handling, raw listener ownership/generations, or history reconstruction. The receipt implementation will intentionally change the observed-start path described above.

# Implemented Evidence Boundary

The subsequent uncommitted Phase 10 implementation adds `app-server/src/compaction_receipts.rs`, Core's prepared handle and `core/src/compaction_submission.rs` enqueue witness, observed-start/read protocol, listener tracking, and versioned initialization. The enqueue witness is recorded synchronously after the existing channel send succeeds; canceled backpressure cannot be reported as accepted. A shared listener-liveness witness prevents a stale weak thread identity from admitting work against a dead observer. Independent review cleared both corrections and the final production boundary.

Verification passed 22 unique tests: 13 receipt registry tests, three real async-channel enqueue tests, two actual WebSocket RPC tests, and four schema consistency tests. The RPC tests cover lost acknowledgement, suppressed lifecycle notifications, disconnect/reconnect, retained terminal proof after listener unload, and invalid or duplicate admission without additional model requests. Hook/replacement consequences are exercised by deterministic lifecycle-event tests, including abortion before and after the completed compaction marker; no additional hook subprocess scenarios were required.

Exact test commands, run from Beryl with two build jobs:

```text
cargo nextest run --manifest-path ../codex-fork/codex-rs/Cargo.toml -p codex-app-server --test compaction_receipts -p codex-core --test compaction_submission -j 2
cargo nextest run --manifest-path ../codex-fork/codex-rs/Cargo.toml -p codex-app-server --test all --test compaction_receipts -E 'test(compaction_receipts_websocket) | binary(compaction_receipts)' -j 2
cargo nextest run --manifest-path ../codex-fork/codex-rs/Cargo.toml -p codex-app-server-protocol --lib -E 'test(schema_fixtures_tests) & !test(write_schema_fixtures_from_env)' -j 2
cargo check --manifest-path ../codex-fork/codex-rs/Cargo.toml -p codex-app-server --tests -j 2
```

Stable and experimental schema generation also passed via the existing ignored `schema_fixtures_tests::write_schema_fixtures_from_env` test with `CODEX_APP_SERVER_SCHEMA_EXPERIMENTAL` set separately to `0` and `1` and an explicit scoped schema root. Generated fixtures and compressed exports are consistent. Beryl-side receipt consumption remains subsequent work, and installed binaries were not replaced.
