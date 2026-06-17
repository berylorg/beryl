# Target Docs

- `doc/design.md`
- `doc/systems/syndic-conversation-history/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `doc/systems/backend-runtime/design.md`
- `doc/systems/transcript-presentation/design.md`
- `doc/features/transcript/design.md`
- `doc/features/conversation-threads/design.md`
- `doc/features/composer/design.md`
- `doc/features/backend-runtime-recovery/design.md`
- `doc/features/threaded-decisions/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`

# Cutover Boundary

- CAS remains the authority for authentication, live execution, enterprise policy enforcement, sandbox behavior, skills, tools, approvals, and runtime-owned operations.
- Syndic becomes the authority for durable transcript history and transcript-view projection for turns captured from Beryl-owned live CAS event streams.
- Selected transcript rendering after cutover must read through the Syndic transcript provider, not CAS historical transcript APIs.
- CAS `thread/turns/list` and full-history reads are legacy or transitional compatibility surfaces only.
- Existing threads that were not captured into Syndic are incomplete from the Syndic transcript provider until a future explicit import or backfill design is approved.
- Missed live events, process loss, or storage failure must produce explicit incomplete or failed captured-history state rather than silently repopulating selected transcript history from CAS.
- Obsolete CAS-history source has been removed from live source locations and archived under `old-code`.
- The project is allowed to stop compiling inside this active rework gap. Broken edges from surviving app/backend code to removed execution-detail, selected-activation, composer-label-scan, response-sanitizer, and thread-history APIs are intentional until replacement checkpoints fill them with target-state Syndic/CAS-live code.
- Temporary cutover shims are allowed only when they connect surviving outer code to new target-state boundaries. They must not import, wrap, call, extend, or preserve archived `old-code`.
- CAS threads are disposable execution projections over Syndic-owned captured history. Surviving code may carry exact CAS ids for live execution, stop, fork, rollback, title, and metadata operations, but must not treat a CAS thread as selected transcript history authority after cutover.
- CAS projection binding state, graph-action reflection outcomes, active-turn mutation rules, fresh context-pack materialization, and stale-thread abandonment are defined by `doc/systems/cas-live-syndic-transcript/design.md`.
- Stale CAS projections are abandoned as execution bindings and are not deleted for cleanup. Archiving may be added only when CAS archive semantics are proven not to affect related threads.

# Reference Snapshot

- `old-doc/features/syndic/design.md`
- `old-code/syndic-storage/doc/design.md`
- `old-code/syndic-storage/src/lib.rs`
- `old-code/beryl-app/src/shell/discovery.rs`
- `old-code/beryl-app/src/shell/selected_thread_activation.rs`
- `old-code/beryl-app/src/shell/execution_detail.rs`
- `old-code/beryl-app/src/shell/execution_detail/history_fallback.rs`
- `old-code/beryl-app/src/shell/composer_image_label_scan.rs`
- `old-code/beryl-app/src/shell/composer_image_labels.rs`
- `old-code/beryl-app/src/shell/composer_image_label_sync.rs`
- `old-code/beryl-backend/src/thread_history.rs`
- `old-code/beryl-backend/src/response_sanitizer.rs`

# Forbidden Local APIs

New CAS-live Syndic capture, transcript-provider, transcript-rendering, composer-label, branch, edit, and selected-activation code must not use these as selected transcript history sources:

- `ManagedBackendSession::list_thread_turns`
- CAS `thread/turns/list`
- `ThreadTurnsListOptions`
- `ExecutionDetail::load_thread_history`
- `ExecutionDetail::prepend_thread_history_page`
- `load_selected_thread_history`
- CAS-history composer image-label scans for captured threads
- Direct `syndic-storage` calls from transcript rendering code
- Archived selected-activation or execution-detail code as the new CAS projection authority
- A new broad `ExecutionDetailState`-style owner that combines active-turn UI state, input fragments, CAS projection binding, transcript history, and rendering authority

# Checklist

- Checkpoint 0: Rework authority and old-code removal.
  - Done: old Syndic docs archived under `old-doc`.
  - Done: old `syndic-storage` doc and source archived under `old-code`.
  - Done: old CAS-history app/backend source archived under `old-code`.
  - Done: obsolete live CAS-history source files removed from `crates/beryl-app/src/shell` and `crates/beryl-backend/src`.
  - Done: obsolete app/backend module registrations and backend public history exports removed.
  - Done: target system, feature, and package docs written.
  - Done: root implementation plan points at this rework.
  - Remaining: inspect and remove or replace surviving dependent code and tests that still name removed APIs.
  - Blocked: Operator review of the removal-first gap before filling it with replacement implementation.
  - Verification: no references to the retired Rust-specific archive directory name remain.
  - Verification: removed live source paths do not exist outside `old-code`.
  - Verification: `cargo check -p syndic-storage` remains green.
  - Verification: full workspace compile is expected to fail until later checkpoints replace the removed APIs.
- Checkpoint 1: Surviving-edge responsibility split and CAS projection boundary.
  - Done: obsolete implementation has been removed before shim work.
  - Remaining: inventory surviving app/backend references to removed selected-activation, execution-detail, composer-label-scan, response-sanitizer, and thread-history APIs.
  - Remaining: split surviving responsibilities across focused selected activation, composer submission, CAS projection binding, active-turn state, transcript-provider, and transcript-host boundaries.
  - Remaining: define live CAS projection binding records and state transitions for valid, stale, unbound, and active bindings.
  - Remaining: implement or stub graph-action classification for no CAS effect, native CAS operation, projection invalidation, and materialize-on-next-run outcomes.
  - Remaining: define exact fresh context-pack materialization contents, ordering, provenance markers, truncation or summarization policy, reference inclusion policy, and CAS request boundary for stale or unbound projections without creating a CAS-history adapter.
  - Remaining: feed the context-pack design back into the owning system and package docs before implementing the materialization path.
  - Remaining: define active-turn mutation gates for immutable accepted input, forbidden incomplete-branching, deleted-active-turn abort/discard, and allowed ancestor edits.
  - Remaining: define stale CAS projection abandonment behavior without deletion cleanup.
  - Remaining: decide which surviving edges need forward-facing cutover shims and name each shim with a removal condition.
  - Remaining: remove obsolete tests or rewrite them against target-state Syndic/CAS-live boundaries.
  - Blocked: Operator review of Checkpoint 0.
  - Verification: no shim imports, wraps, calls, extends, or preserves `old-code`.
  - Verification: no live project manifest, source registry, dependency declaration, entry point, test, script, or build configuration references `old-code`.
- Checkpoint 2: `syndic-storage` fjall API and schema.
  - Done: target storage boundary docs exist.
  - Remaining: define durable conversation, turn, source-event, resource, projection, revision, and cursor records.
  - Remaining: define write-batch and crash-recovery behavior.
  - Remaining: keep provider calls, auth, CAS execution, and renderer code out of the crate.
  - Blocked: Checkpoint 1.
  - Verification: storage crate tests cover bounded reads, idempotent events, incomplete turns, stale projections, and resource range errors.
- Checkpoint 3: CAS live event ingestion.
  - Done: CAS remains the execution and policy authority in target docs.
  - Remaining: capture accepted user input, CAS live events, assistant deltas, terminal states, metadata, resources, and failure records into Syndic.
  - Remaining: reject or mark incomplete captures when durable admission cannot be proven.
  - Remaining: record missed live events, stream loss, and storage failures explicitly.
  - Blocked: Checkpoint 2.
  - Verification: ingestion tests cover durable admission before composer clear, terminal state, stream loss, duplicate events, and crash recovery.
- Checkpoint 4: Storage-backed transcript provider.
  - Done: transcript presentation system docs define provider and residency boundaries.
  - Remaining: replace fixture-only provider behavior with bounded reads from Syndic projections.
  - Remaining: expose incomplete-history and resource-readiness states explicitly.
  - Remaining: keep render paths isolated from direct storage calls.
  - Blocked: Checkpoint 2.
  - Verification: provider tests cover cursor pages, stale revisions, missing resources, incomplete histories, and bounded reads.
- Checkpoint 5: Selected-thread activation cutover.
  - Done: CAS metadata-only resume remains the target execution binding.
  - Remaining: prepare selected transcript state from the Syndic provider.
  - Remaining: remove CAS paginated history from selected transcript activation.
  - Remaining: define startup and selector behavior for threads with no captured Syndic history.
  - Blocked: Checkpoints 3 and 4.
  - Verification: activation tests prove no CAS historical transcript request is made for captured selected threads.
- Checkpoint 6: Composer, copy, quote, branch, and edit proof.
  - Done: system and feature docs assign proof to resident Syndic provenance and owning-history frontiers.
  - Remaining: use resident Syndic provenance and owning-history label frontiers.
  - Remaining: keep image paste unavailable when captured history is incomplete.
  - Remaining: route branch and edit mutations through CAS execution primitives and Syndic history updates.
  - Done: edit-as-branch, edit replacement with detached tails and no immediate Syndic garbage collection, and CAS compaction marker turn/item placement target policy are documented in the owning system docs.
  - Blocked: Checkpoints 3, 4, and 5.
  - Verification: composer, quote, copy, branch, and edit tests cover incomplete history, stable provenance, labels, rollback, replacement turns, and missed-event states.
- Checkpoint 7: Cleanup and verification.
  - Done: initial obsolete source removal completed in Checkpoint 0.
  - Remaining: remove any temporary cutover shims after target implementation replaces them.
  - Remaining: verify renderer, selected activation, live streaming, restart, missed-event, image-label, branch, edit, copy, quote, activity, and transcript behavior.
  - Blocked: Checkpoints 1 through 6.
  - Verification: forbidden API scan has no live matches outside archived `old-code` and explicitly allowed legacy backend compatibility surfaces.
  - Verification: full workspace checks pass after the target implementation closes the rework gap.
