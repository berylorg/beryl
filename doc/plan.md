# Scope

Active rework: `doc/rework/cas-live-syndic-transcript/REWORK.md`.

Replace CAS-backed selected transcript history with CAS-live capture into Syndic storage and storage-backed Syndic transcript rendering. CAS remains live execution, authentication, enterprise policy, sandbox, tools, approvals, and runtime owner during this rework.

# Phase 1: Rework Authority Realignment And Old-Code Removal (wip)

Status: blocked on Operator review of the intentional removal-first gap before replacement implementation.

- Done: updated the active rework tracker to use `old-code` rather than Rust-specific archive language.
- Done: reclassified Syndic durable history, CAS-live capture, backend runtime internals, Codex-compatible agent-layer constraints, and transcript presentation internals into `doc/systems`.
- Done: trimmed transcript feature authority back to user-visible transcript behavior.
- Done: archived obsolete docs under `old-doc`.
- Done: archived obsolete source under `old-code`.
- Done: removed obsolete live CAS-history source files from `beryl-app` and `beryl-backend`.
- Done: removed obsolete app/backend module registrations and backend public history exports.
- Done: recorded that full workspace compilation is expected to fail until replacement checkpoints close the gap.
- Remaining: Operator review of the removal boundary, forbidden APIs, system-doc authority split, and next checkpoint.
- Edge case: surviving live source and tests still name removed APIs; that is the visible rework gap, not permission to restore archived code.
- Verification: confirm no references to the retired Rust-specific archive directory name remain.
- Verification: confirm removed live source paths exist only under `old-code`.
- Verification: `cargo check -p syndic-storage` should remain green.

# Phase 2: Surviving-Edge Responsibility Split And CAS Projection Boundary (pending)

- Inventory surviving app/backend references to removed selected-activation, execution-detail, composer-label-scan, response-sanitizer, and thread-history APIs.
- Split surviving responsibilities across selected activation, composer submission, CAS projection binding, active-turn state, transcript-provider, and transcript-host boundaries.
- Define CAS projection binding records and graph-action reflection outcomes for valid, stale, unbound, and active bindings.
- Define active-turn mutation gates, exact context-pack materialization contents and policy, and stale CAS projection abandonment behavior.
- Name any forward-facing cutover shims with explicit removal conditions in `REWORK.md`.
- Remove obsolete tests or rewrite them against target-state Syndic/CAS-live boundaries.
- Verify no shim imports, wraps, calls, extends, or preserves `old-code`.

# Phase 3: Syndic Storage API And Fjall Schema (pending)

- Implement durable conversation, turn, source-event, resource, projection, revision, and cursor records.
- Implement write batches, crash-recovery markers, and incomplete-history states.
- Keep provider calls, auth, execution, and rendering out of `syndic-storage`.

# Phase 4: CAS Live Event Ingestion (pending)

- Capture accepted user input and live CAS turn events into Syndic.
- Persist assistant streaming, terminal states, metadata, resources, and failure records.
- Require durable admission before composer clear and transcript mutation.

# Phase 5: Storage-Backed Transcript Provider (pending)

- Replace fixture-backed provider behavior with bounded Syndic projection reads.
- Expose cursor pages, resident projection records, resource metadata, and explicit incomplete-history states.
- Keep render paths isolated from direct storage calls.

# Phase 6: Selected Activation And Composer Cutover (pending)

- Keep CAS metadata-only resume for exact backend thread activation.
- Prepare selected transcript state from the Syndic provider instead of CAS paginated history.
- Move image-label authority, copy, quote, branch, and edit proof to resident Syndic provenance and owning-history frontiers.
- Preserve the documented edit-as-branch, edit replacement with detached tails and no immediate Syndic garbage collection, and CAS compaction marker policies while implementing branch, edit, copy, quote, and label proof.

# Phase 7: Cleanup And Verification (pending)

- Remove temporary cutover shims after replacement code closes the gap.
- Verify live streaming, restart recovery, missed-event handling, selected activation, image labels, branch, edit, copy, quote, activity, and transcript renderer behavior.
