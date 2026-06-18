# Scope

Active rework: `doc/rework/workspace-syndic-thread-catalog/REWORK.md`.

Rework target: Beryl shell thread catalogs, thread selectors, workspace restore, graph link-thread menus, breadcrumbs, navigation history, and title rows must be built from Beryl workspace-owned Syndic conversation-view refs joined with Syndic history-derived summaries.

Domain split:

- Syndic owns historical conversation data and history-derived summaries.
- Beryl workspace storage owns selected view, workspace membership, runtime/member bindings, GUI-local titles, semantic graph refs, navigation history, and other GUI state.
- CAS is only the live execution stream/control source for approved projections and must not be used as catalog, selector, restore, title, branch-tree, workspace-membership, or link-menu authority.

Architectural rework standing rule: obsolete CAS-backed catalog and restore code must be removed or quarantined under the active `old-code` archive rather than preserved behind transition adapters.

# Phase 1: Rework Document Review (wip)

- Done: corrected target docs to remove CAS thread-list and metadata-read authority from thread selector, catalog, restore, title, and link-menu behavior.
- Done: created `doc/rework/workspace-syndic-thread-catalog/REWORK.md` with target docs, cutover boundary, forbidden local APIs, and checkpointed replacement work.
- Remaining: Operator review of the new rework tracker before implementation starts.
- Edge case: storage engine unification is intentionally out of scope for this rework; logical domains must remain separate even if `redb` and `fjall` are unified later.
- Edge case: existing CAS threads without Syndic view registration must not appear through a compatibility inventory while the replacement is incomplete.
- Verification: confirm the rework tracker forbids CAS catalog/restore authority and does not authorize transition adapters.
- Resumable milestone: waiting for Operator review of `doc/rework/workspace-syndic-thread-catalog/REWORK.md`.

# Phase 2: Remove CAS-Backed Catalog And Restore Surfaces (pending)

- Remove or quarantine app-side backend-list inventory workers, selector tests, restore paths, and title paths that treat CAS metadata as catalog authority.
- Remove startup and selector refresh paths that call backend thread-list or metadata-read methods to populate visible thread rows.
- Keep the selector/restore gap visible until workspace-plus-Syndic catalog replacement lands.
- Verification: live app source and tests do not call CAS list/read metadata APIs for shell catalog, selector, restore, title, breadcrumbs, graph links, or navigation paths.

# Phase 3: Workspace View Registration And Syndic Summaries (pending)

- Add workspace-owned registration of Syndic conversation-view refs, active selected view, runtime/member binding, title metadata, and catalog status.
- Add bounded Syndic history summary reads for catalog joins without storing GUI state in Syndic.
- Make workspace restore require a registered Syndic view ref or fall back to pending-new-thread.
- Verification: restore cannot select a CAS thread id without a workspace-registered Syndic view.

# Phase 4: Workspace-Plus-Syndic Catalog Cutover (pending)

- Implement catalog refresh as workspace refs joined with Syndic summaries.
- Rewire selector columns, branch columns, breadcrumbs, graph link-thread menus, navigation history, and activation to Syndic view identities.
- Rewire new, branch, edit, title, and image-label readiness workflows to update catalog state without backend inventory refresh.
- Verification: opening the selector never calls CAS and displays only workspace-registered Syndic views.

# Phase 5: Backend Cleanup And Full Verification (pending)

- Remove or quarantine backend protocol surfaces that exist only for shell catalog, selector, title, or restore.
- Keep only CAS live execution/control APIs required by approved Syndic projection operations.
- Add source-boundary tests forbidding CAS catalog authority in shell paths.
- Verification: `cargo fmt --check`, `cargo check --workspace`, focused catalog/source-boundary tests, and `cargo nextest run --workspace --no-fail-fast`.
