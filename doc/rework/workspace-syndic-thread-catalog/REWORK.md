# Status

Completed historical rework whose workspace-era target was superseded by the active Beryl-home
replacement. It is not live target authority and must not be resumed. Current catalog ownership,
paging, search, shell integration, and bounded-resource work belongs to
`doc/rework/beryl-home/REWORK.md` and its target documents.

# Target Docs

- `doc/features/conversation-threads/design.md`
- `doc/features/composer/design.md`
- `doc/features/transcript/design.md`
- `doc/systems/syndic-conversation-history/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `doc/systems/backend-runtime/design.md`
- `doc/systems/transcript-presentation/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-model/doc/design.md`

# Cutover Boundary

- Syndic owns historical conversation data: turns, source events, canonical items, transcript views, branch/view relationships, projection records, resources, provenance, completeness state, and bounded history-derived summaries.
- Beryl workspace storage owns GUI and workspace state: selected conversation view, workspace membership, runtime/member bindings, manual and generated title metadata, semantic graph refs, navigation history, draft state, and user preferences.
- The thread selector, thread strip, graph link-thread menus, breadcrumbs, workspace restore, and navigation history must be built from workspace-registered Syndic conversation-view refs joined with bounded Syndic history summaries.
- CAS is not a thread catalog, selector, restore, title, branch-tree, workspace-membership, or link-menu authority.
- CAS may appear only as live execution projection metadata stored under Syndic/workspace-owned records and as the live stream/control source for approved execution operations.
- CAS thread-list, metadata-only thread-read, backend thread-name, backend fork-parent, backend updated-time, and backend working-directory list metadata are obsolete for shell catalog and restore surfaces.
- Existing CAS threads that were never captured into Syndic and never registered as workspace-owned Syndic conversation views do not appear in Beryl selectors or workspace restore.
- This rework does not define automatic CAS import or backfill. A future import design must be explicit and must write target-state Syndic history plus workspace refs instead of preserving CAS as a live catalog.
- This rework does not unify storage engines. If `redb` and `fjall` are later unified, logical domains must remain separate: Syndic history records must not own GUI state.
- Temporary transition adapters that keep CAS inventory alive behind a Syndic-shaped facade are forbidden. Remove or quarantine obsolete code instead.
- `crates/beryl-app/src/member_thread_inventory.rs` and `crates/beryl-app/src/shell/member_thread_inventory.rs` are the live workspace-plus-Syndic catalog boundary. They must not ingest `ThreadSummary`, call CAS, read backend names, read backend fork parents, retain backend inventory snapshots, or publish selector rows from backend inventory.

# Reference Snapshot

- Obsolete backend-list inventory code is archived under `doc/rework/workspace-syndic-thread-catalog/old-code/crates/beryl-app/src/member_thread_inventory.rs`.
- Obsolete backend-list inventory worker code is archived under `doc/rework/workspace-syndic-thread-catalog/old-code/crates/beryl-app/src/shell/member_thread_inventory.rs`.
- Obsolete selector, graph-link, navigation, breadcrumb, workflow-recovery, and inventory tests that seeded rows from backend summaries are archived under `doc/rework/workspace-syndic-thread-catalog/old-code/crates/beryl-app/tests/`.

# Forbidden Local APIs

New workspace thread catalog, selector, restore, graph-link, breadcrumb, title, and activation code must not use these as thread catalog or restore authority:

- `ManagedBackendSession::list_threads`
- `ManagedBackendSession::list_threads_with_options`
- `ManagedBackendSession::read_thread` for metadata-only catalog or restore
- `ThreadListOptions`
- CAS `thread/list`
- CAS `thread/read` metadata rows
- backend thread-name metadata as selector title authority
- backend fork-parent metadata as branch-tree authority
- backend updated-time metadata as selector ordering authority
- backend working-directory list metadata as workspace grouping authority
- `member_thread_inventory` workers that build rows from backend thread lists
- workspace restore from a persisted CAS thread id without a workspace-registered Syndic conversation-view ref

# Checklist

- Checkpoint 0: Authority, review, and visible gap definition.
  - [x] Target docs now split Syndic historical data from Beryl workspace GUI state.
  - [x] Target docs now forbid CAS thread-list and metadata-read APIs as selector, restore, title, or catalog authority.
  - [x] Target docs preserve the rule that storage-engine unification is separate from logical domain ownership.
  - [x] This tracker defines the removal-first cutover boundary for the next architectural rework.
  - [x] Operator reviewed this `REWORK.md`.
  - [x] Previous CAS-live Syndic transcript rework is complete and `doc/plan.md` is available for the next active rework.
  - [x] Reviewed this tracker against the target docs before source edits.
- Checkpoint 1: Remove obsolete CAS-backed catalog and restore surfaces.
  - [x] Archived app-side backend-list inventory workers, snapshot builders, selector tests, and restore paths that treated CAS thread ids or metadata rows as catalog authority.
  - [x] Removed startup and selector refresh paths that called backend thread-list methods to populate visible thread rows.
  - [x] Removed backend-name and backend-fork-parent metadata from selector title and branch-tree authority by replacing the live inventory with an empty non-CAS catalog holder.
  - [x] Removed or quarantined tests whose assertions depended on CAS-discovered selector rows.
  - [x] Left the selector/restore behavior gap visible as an empty catalog holder during checkpoint 1 rather than hiding it behind a CAS compatibility adapter.
  - [x] Verified live app source and tests do not call `list_threads`, `list_threads_with_options`, or `ThreadListOptions` for shell catalog, selector, restore, or title paths.
  - [x] Verified live selector and restore tests do not seed rows by pretending CAS thread-list data is authoritative.
- Checkpoint 2: Add workspace-owned conversation-view registration.
  - [x] Update `beryl-model` workspace conversation state to register Syndic conversation-view refs, active selected view, runtime/member binding, title metadata, and catalog status without backend thread-list summaries.
  - [x] Update workspace persistence to store and load the new workspace-owned registration records.
  - [x] Treat obsolete persisted backend-thread-only registrations as invalid for catalog and restore; they must not produce selector rows or selected-thread restore unless a future explicit import design writes target-state Syndic history plus workspace refs.
  - [x] Ensure selected-thread restore requires a workspace-registered Syndic view ref and otherwise opens the pending-new-thread surface.
  - [x] Verify workspace restore cannot select a CAS thread id that lacks a Syndic view registration.
  - [x] Verify semantic graph thread refs point to workspace-registered Syndic views, not backend-discovered thread rows.
- Checkpoint 3: Add Syndic history summaries for catalog joins.
  - [x] Extend `syndic-storage` with bounded conversation/view summary reads for last captured activity, title candidates, completeness, branch/view parentage, and projection binding summary.
  - [x] Keep selected view, manual/generated titles, member bindings, semantic refs, and navigation state out of Syndic storage.
  - [x] Update the storage-backed provider or adjacent history boundary so catalog refresh can read summaries without invoking transcript rendering or CAS.
  - [x] Verify history summary APIs expose only history-derived facts and never store GUI state.
  - [x] Verify summary reads are bounded and do not scan full transcript bodies for selector rendering.
- Checkpoint 4: Replace selector, restore, links, breadcrumbs, and navigation with workspace-plus-Syndic catalog.
  - [x] Implement catalog refresh as a join of workspace-registered view refs with Syndic summaries.
  - [x] Make selector columns, branch columns, breadcrumbs, link-thread menus, and navigation history use Syndic view identities.
  - [x] Keep open-selector projections stable while background catalog refresh completes.
  - [x] Make existing-thread activation prepare the selected transcript from the Syndic provider without CAS metadata-only resume as catalog proof.
  - [x] Verify opening the thread selector never calls CAS and shows only workspace-registered Syndic views.
  - [x] Verify workspace startup restores only a registered Syndic view or falls back to pending-new-thread.
  - [x] Verify branch columns and breadcrumbs are derived from Syndic/workspace relationships, not backend fork metadata.
- Checkpoint 5: Rewire creation, branch, edit, and title publication to register workspace views.
  - [x] New thread creation must create or bind a Syndic conversation view and register it in workspace storage before it can appear in the selector.
  - [x] Branch workflows must publish durable Syndic branch state and workspace registration before catalog visibility.
  - [x] Edit replacement must update Syndic history and catalog summaries without backend list refresh.
  - [x] Title generation must persist generated workspace title metadata and refresh the catalog without publishing or reading CAS thread-name authority.
  - [x] Composer image-label readiness must key existing-thread readiness by Syndic view identity.
  - [x] Verify successful new, branch, edit, and title workflows update selector rows without backend inventory refresh.
  - [x] Verify failed branch/bootstrap/title workflows leave no partial workspace catalog rows.
- Checkpoint 6: Backend and test cleanup.
  - [x] Removed and quarantined live backend protocol surfaces that existed only for shell catalog, selector, title, restore, or metadata-only activation proof.
  - [x] Kept only CAS live execution/control APIs that are still required by approved Syndic projection operations.
  - [x] Replaced source-boundary tests so CAS list/read metadata methods are forbidden in shell catalog paths.
  - [x] Ran focused app/model/storage/backend tests and then full workspace verification.
  - [x] Verified source scans find no live shell catalog use of CAS thread-list, metadata-read, backend title, backend fork-parent, backend updated-time, or backend working-directory list metadata.
  - [x] Verified live project manifests, tests, and source do not reference this rework's `old-code`.
  - [x] Verified `cargo fmt --check`, `cargo check --workspace`, and `cargo nextest run --workspace --no-fail-fast` pass after the rework closes.
  - [x] Verified `cargo nextest run -p beryl-app --test transcript_rework_source_boundary --no-fail-fast`.
  - [x] Verified `cargo nextest run -p beryl-backend --test turn_protocol --test managed_websocket --test launch_and_protocol --no-fail-fast`.
  - [x] Verified `cargo nextest run -p beryl-app --test member_thread_inventory --test thread_navigation --test thread_selection --test composer_image_label_frontier --test transcript_branch_edit_target --test syndic_live_ingestion --no-fail-fast`.
  - [x] Verified `cargo nextest run -p beryl-model -p syndic-storage --no-fail-fast`.
- Checkpoint 7: Completion-review title authority and catalog bounding fixes.
  - [x] Removed CAS backend thread-name writes from Beryl automatic title generation.
  - [x] Persisted generated thread titles through Beryl workspace-owned generated title metadata.
  - [x] Removed backend-name checks from automatic title eligibility so CAS names cannot suppress Beryl-generated titles.
  - [x] Fixed catalog summary bounding so Syndic history recency cannot be excluded before summary ordering in large workspaces.
  - [x] Added source-boundary tests forbidding CAS title authority in shell/catalog paths.
  - [x] Verified focused title/catalog/source-boundary tests pass.
  - [x] Verified `cargo fmt --check`, `cargo check --workspace`, and `cargo nextest run --workspace --no-fail-fast` pass after review fixes.
  - [x] Verified source scans find no live app/backend/model source use of deleted CAS title-authority APIs, backend title compatibility helpers, or automatic-title/backend-name suppression state.
- Checkpoint 8: Completion-review backend-name authority leaks.
  - [x] Removed `ThreadSummary.name` from resident branch materialization so CAS backend thread names cannot become Syndic title candidates or catalog row titles.
  - [x] Removed `ThreadSummary.name` from graph-started and decision-branch semantic graph thread-ref labels.
  - [x] Added focused regression or source-boundary tests preventing backend summary names from entering Syndic title records, catalog titles, or graph thread-ref labels.
  - [x] Verified focused branch/graph/source-boundary tests pass.
  - [x] Verified `cargo fmt --check`, `cargo check --workspace`, and `cargo nextest run --workspace --no-fail-fast` pass after follow-up fixes.
  - [x] Verified source scans find no live branch, graph-start, or decision-branch source passing backend summary names into Syndic titles or graph-ref labels.
- Checkpoint 9: Completion-review pending activation label leak.
  - [x] Remove `ThreadSummary.name` from resident branch and decision-branch pending activation labels.
  - [x] Resolve branch switch labels from Beryl workspace-owned title metadata after registration, or use the local untitled fallback when no workspace title exists.
  - [x] Add source-boundary regression coverage preventing backend summary names from entering thread-selector activation targets.
  - [x] Verify focused branch completion/source-boundary tests pass.
  - [x] Verify `cargo fmt --check`, `cargo check --workspace`, and `cargo nextest run --workspace --no-fail-fast` pass after follow-up fixes.
  - [x] Verify source scans find backend summary name label/title leak patterns only inside source-boundary guard-test literals.
- Checkpoint 10: Completion-review recovery and backend-name snapshot cleanup.
  - [x] Remove `ThreadSummary.name` from blocked-workspace recovery activation labels.
  - [x] Remove obsolete backend-name snapshot storage from workspace thread registrations.
  - [x] Remove backend-name exposure from graph workspace-state tool output.
  - [x] Stop copying backend summary names into `RegisteredConversationThread` values created from live backend summaries.
  - [x] Remove `ThreadSummary.name` from tool-activity non-subagent display fallback.
  - [x] Add source-boundary coverage preventing backend names from entering recovery activation labels or workspace-owned registration/tool surfaces.
  - [x] Verify focused model/app tests pass.
  - [x] Verify `cargo fmt --check`, `cargo check --workspace`, source scans, and `cargo nextest run --workspace --no-fail-fast` pass after follow-up fixes.
- Checkpoint 11: Completion-review resident branch bootstrap title leak.
  - [x] Remove `selected_thread.name` from resident branch bootstrap parent-title text.
  - [x] Resolve resident branch bootstrap parent titles from Beryl workspace-owned title metadata or the local untitled fallback.
  - [x] Add source-boundary coverage preventing `selected_thread.name` from entering resident branch bootstrap text.
  - [x] Update stale model crate docs that still describe backend names as retained runtime metadata.
  - [x] Verify focused branch/source-boundary tests pass.
  - [x] Verify `cargo fmt --check`, `cargo check --workspace`, source scans, and `cargo nextest run --workspace --no-fail-fast` pass after follow-up fixes.
- Checkpoint 12: Completion-review display-label helper coverage.
  - [x] Add source-boundary coverage for `selected_thread_display_label` so the helper used by recovery activation cannot read `ThreadSummary.name`.
  - [x] Verify focused source-boundary tests and targeted source scans pass.
