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
  - Done: target docs now split Syndic historical data from Beryl workspace GUI state.
  - Done: target docs now forbid CAS thread-list and metadata-read APIs as selector, restore, title, or catalog authority.
  - Done: target docs preserve the rule that storage-engine unification is separate from logical domain ownership.
  - Done: this tracker defines the removal-first cutover boundary for the next architectural rework.
  - Remaining: Operator review of this `REWORK.md`.
  - Ready: previous CAS-live Syndic transcript rework is complete and `doc/plan.md` is available for the next active rework.
  - Verification: review this tracker against the target docs before any source edits.
- Checkpoint 1: Remove obsolete CAS-backed catalog and restore surfaces.
  - Remaining: archive or remove app-side backend-list inventory workers, snapshot builders, selector tests, and restore paths that treat CAS thread ids or metadata rows as catalog authority.
  - Remaining: remove startup and selector refresh paths that call backend thread-list or metadata-read methods to populate visible thread rows.
  - Remaining: remove backend-name and backend-fork-parent metadata from selector title and branch-tree authority.
  - Remaining: remove or quarantine tests whose assertions depend on CAS-discovered selector rows.
  - Remaining: leave the resulting selector/restore compile or behavior gap visible until the workspace-plus-Syndic catalog replacement lands.
  - Verification: live app source and tests do not call `list_threads`, `list_threads_with_options`, or metadata-only `read_thread` for shell catalog, selector, restore, or title paths.
  - Verification: live selector and restore tests do not seed rows by pretending CAS thread-list data is authoritative.
- Checkpoint 2: Add workspace-owned conversation-view registration.
  - Remaining: update `beryl-model` workspace conversation state to register Syndic conversation-view refs, active selected view, runtime/member binding, title metadata, and catalog status without backend thread-list summaries.
  - Remaining: update workspace persistence to store and load the new workspace-owned registration records.
  - Remaining: treat obsolete persisted backend-thread-only registrations as invalid for catalog and restore; they must not produce selector rows or selected-thread restore unless a future explicit import design writes target-state Syndic history plus workspace refs.
  - Remaining: ensure selected-thread restore requires a workspace-registered Syndic view ref and otherwise opens the pending-new-thread surface.
  - Verification: workspace restore cannot select a CAS thread id that lacks a Syndic view registration.
  - Verification: semantic graph thread refs point to workspace-registered Syndic views, not backend-discovered thread rows.
- Checkpoint 3: Add Syndic history summaries for catalog joins.
  - Remaining: extend `syndic-storage` with bounded conversation/view summary reads for last captured activity, title candidates, completeness, branch/view parentage, and projection binding summary.
  - Remaining: keep selected view, manual/generated titles, member bindings, semantic refs, and navigation state out of Syndic storage.
  - Remaining: update the storage-backed provider or adjacent history boundary so catalog refresh can read summaries without invoking transcript rendering or CAS.
  - Verification: history summary APIs expose only history-derived facts and never store GUI state.
  - Verification: summary reads are bounded and do not scan full transcript bodies for selector rendering.
- Checkpoint 4: Replace selector, restore, links, breadcrumbs, and navigation with workspace-plus-Syndic catalog.
  - Remaining: implement catalog refresh as a join of workspace-registered view refs with Syndic summaries.
  - Remaining: make selector columns, branch columns, breadcrumbs, link-thread menus, and navigation history use Syndic view identities.
  - Remaining: keep open-selector projections stable while background catalog refresh completes.
  - Remaining: make existing-thread activation prepare the selected transcript from the Syndic provider without CAS metadata-only resume as catalog proof.
  - Verification: opening the thread selector never calls CAS and shows only workspace-registered Syndic views.
  - Verification: workspace startup restores only a registered Syndic view or falls back to pending-new-thread.
  - Verification: branch columns and breadcrumbs are derived from Syndic/workspace relationships, not backend fork metadata.
- Checkpoint 5: Rewire creation, branch, edit, and title publication to register workspace views.
  - Remaining: new thread creation must create or bind a Syndic conversation view and register it in workspace storage before it can appear in the selector.
  - Remaining: branch workflows must publish durable Syndic branch state and workspace registration before catalog visibility.
  - Remaining: edit replacement must update Syndic history and catalog summaries without backend list refresh.
  - Remaining: title generation must persist generated workspace title metadata and refresh the catalog without publishing or reading CAS thread-name authority.
  - Remaining: composer image-label readiness must key existing-thread readiness by Syndic view identity.
  - Verification: successful new, branch, edit, and title workflows update selector rows without backend inventory refresh.
  - Verification: failed branch/bootstrap/title workflows leave no partial workspace catalog rows.
- Checkpoint 6: Backend and test cleanup.
  - Remaining: remove or quarantine live backend protocol surfaces that exist only for shell catalog, selector, title, or restore.
  - Remaining: keep only CAS live execution/control APIs that are still required by approved Syndic projection operations.
  - Remaining: replace source-boundary tests so CAS list/read metadata methods are forbidden in shell catalog paths.
  - Remaining: run focused app/model/storage/backend tests and then full workspace verification.
  - Verification: source scan finds no live shell catalog use of CAS thread-list, metadata-read, backend title, backend fork-parent, backend updated-time, or backend working-directory list metadata.
  - Verification: live project manifests, tests, and source do not reference this rework's `old-code`.
  - Verification: `cargo fmt --check`, `cargo check --workspace`, and `cargo nextest run --workspace --no-fail-fast` pass after the rework closes.
