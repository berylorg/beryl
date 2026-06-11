# Scope

Implement the transcript resident-window architecture defined by `doc/features/transcript/design.md`, `crates/beryl-app/doc/design.md`, and `doc/app-server-contract.md`.

The implementation rewrites transcript history loading so visible transcript rows are backed only by resident full-detail turns. Existing-thread activation and workspace-open restoration must prepare a coherent resident tail window before publishing the selected transcript. Scroll loading and release must be owned by a configurable residency controller with policy budgets and diagnostics. Scrolling past resident data must clamp and request policy-allowed loading instead of exposing skeleton, unloaded, or synthetic loading transcript rows.

The old architecture is not an allowed compatibility path: do not preserve renderer/prepaint/deferred detail scheduling, per-turn skeleton rows, `Loading transcript details...` transcript content, released-history placeholder rows as scrollable narrative, hardcoded detail overscan constants, or migration adapters that keep the old model alive.

Latest resumable milestone: planning complete, no implementation phase has started.

# Phase 1: Replace The Core Residency Model (wip)

Hard task: Replace the current `TranscriptHistoryWindow` plus `TranscriptTurnDetailCache` skeleton/detail split with a single transcript residency model that separately owns non-rendered turn index metadata, resident full-detail turns, policy state, pins, in-flight requests, release decisions, and content-free diagnostics.

Implementation notes: Target `crates/beryl-app/src/shell/transcript_history.rs`, `crates/beryl-app/src/shell/transcript_history/detail_cache.rs`, `crates/beryl-app/src/shell/transcript_turn_detail.rs`, and their tests first. The new model must preserve backend turn ids, ordering, cursor metadata, source positions, measured-height estimates, and exact current-tail/oldest-known facts without representing missing detail as renderable rows. `itemsView = "notLoaded"` may only enter index/planning records. `itemsView = "full"` responses may admit only turns selected by policy or explicit pins.

Edge cases: duplicate or overlapping page responses, empty latest history, operational-only turns that project to no transcript row, full responses returning more turns than requested, stale in-flight responses after thread switch, released turns with retained measurements, pinned turns outside the viewport, and budget-limited windows smaller than the target margin.

Verification: Add focused unit tests in `crates/beryl-app/tests/transcript_history.rs` for index-only pages, full-page admission, over-return release, stale response rejection, pin retention, budget shrinkage, and the absence of skeleton/loading transcript records. Keep existing generated-image sanitization guarantees.

Resumable milestone: The residency model can be tested without shell rendering, and no test or source helper needs `TranscriptTurnSkeleton` or `HISTORY_DETAIL_LOADING_TEXT`.

# Phase 2: Prepare Resident Windows During Activation (pending)

Hard task: Move existing-thread activation, startup workspace restoration, foreground branch switch, and closed decision branch opening to a prepared-window contract that loads enough resident full-detail history before publishing the selected transcript state.

Implementation notes: Target `crates/beryl-app/src/shell/thread_activation.rs`, `crates/beryl-app/src/shell/turn_worker.rs`, `crates/beryl-app/src/shell/discovery.rs`, `crates/beryl-app/src/shell/lifecycle.rs`, `crates/beryl-app/src/shell/thread_history_worker.rs`, branch activation paths, and decision archive paths. Activation workers should resume metadata, validate the execution target, build the initial resident policy request at the tail, issue bounded `thread/turns/list itemsView = "full"` requests as needed, resolve image sources for admitted resident turns, and return a prepared activation package. Shell publication must remain atomic: selected chrome, resident transcript rows, list state, and activation viewport apply together.

Edge cases: activation failure after metadata resume but before resident history, empty or operational-only thread tail, initial policy satisfied by fewer turns because of byte budget, backend returning partial full pages, generated-image path resolution failures, activation superseded by a newer thread selection, and workspace-open restoration with no selected thread.

Verification: Add worker-level tests for prepared activation request sequencing and shell-source tests proving activation does not call renderer or scroll callbacks to finish required detail loading. Existing pending-activation tests must assert previous coherent transcript remains visible until the prepared package applies.

Resumable milestone: Opening or restoring a selected existing thread never creates a visible transcript row from `notLoaded` data and never needs a post-paint detail load to make the initial viewport coherent.

# Phase 3: Route Scroll And Rendering Through Residency (pending)

Hard task: Rewire transcript rendering, list scrolling, page commands, and history workers so the residency controller owns scroll-boundary loading, clamping, resident extension, resident release, and presentation updates.

Implementation notes: Target `crates/beryl-app/src/shell.rs`, `crates/beryl-app/src/shell/render/transcript.rs`, `crates/beryl-app/src/shell/transcript_presentation.rs`, `crates/beryl-app/src/shell/transcript_presentation_reconcile.rs`, `crates/beryl-app/src/shell/virtual_list/*`, and the history worker. Rendering may report viewport facts, measurements, media preload ranges, and frame diagnostics, but must not schedule transcript detail loads. When wheel, page, or scrollbar input would move beyond resident boundaries, clamp at the resident edge, notify residency of the requested direction, and extend scrollable range only after resident full-detail rows are ready. Release must remove rows from presentation when they leave residency, preserving measurements for anchor reconciliation without leaving placeholder transcript content.

Edge cases: manual scroll intent while clamped, repeated wheel/page input while a load is in flight, scrollbar drag across a resident edge, resize/theme/font remeasurement near a boundary, selected anchor row released by policy, live-turn anchoring interacting with resident-history changes, virtual trailing final-answer runway at the tail, and source-position totals that remain unknown while older index pages are not complete.

Verification: Replace old source tests that assert prepaint/detail scheduling with tests that assert renderer non-ownership, scroll clamp behavior, resident-edge load requests, stable anchor preservation, no placeholder rows after release, and bounded rendered row count independent of indexed history size.

Resumable milestone: Scrolling can no longer reveal skeleton or unloaded transcript content, and all history-extension decisions flow through one residency controller entry point.

# Phase 4: Enforce Elastic Policy Budgets And Diagnostics (pending)

Hard task: Make transcript residency policy configurable and budget-aware across turn count, viewport-height margins, cold-release hysteresis, resident byte estimates, in-flight request limits, request priority, pins, media state, Markdown caches, code-panel projections, and diagnostics.

Implementation notes: Target policy structs and defaults in the shell residency module, retained-state snapshots in `crates/beryl-app/src/memory_diagnostics.rs`, diagnostic dynamic tools in `crates/beryl-app/src/diagnostic_dynamic_tools.rs`, transcript frame metrics, visible-media diagnostics, Markdown/media/cache accounting, and UI pin integrations from context menus, edit mode, media actions, and active turns. The default policy may encode the current desired runway, but it must be data, not hardcoded scheduling logic. Byte accounting can start with conservative estimates for resident turn payloads, text, item counts, Markdown/cache entries, generated-image bytes, media presentation state, and code-panel projection state.

Edge cases: five huge turns exceeding the desired viewport margin, saved-path-backed generated images that should not count durable file bytes, unreadable saved paths falling back to retained bytes, pins that would exceed ordinary resident budgets, diagnostics truncation, policy changes during an in-flight request, and memory diagnostics when no transcript is selected.

Verification: Add unit tests for policy outcomes under competing viewport and byte limits, in-flight caps, priority order, pin retention, release hysteresis, and content-free diagnostic serialization. Update retained-state tests to use residency terminology instead of skeleton/detail cache counters.

Resumable milestone: Operators can inspect why the resident runway is smaller than the target margin, and changing policy values does not require rewriting scroll or render code.

# Phase 5: Remove Old Architecture And Complete Verification (pending)

Hard task: Delete the old detail-loading architecture, stale tests, stale diagnostics, stale labels, and source paths that assume skeleton rows or renderer-owned detail loading, then run the full targeted verification suite.

Implementation notes: Remove or rename `thread_turn_detail_worker.rs`, obsolete `TranscriptDetailLoad*` diagnostics, `TranscriptTurnSkeleton`, `HISTORY_DETAIL_LOADING_TEXT`, placeholder rendering helpers, released-history placeholder paths, and old tests that preserve those strings or flows. Update code owners around branch/edit/title/update menus so they require resident parent turns. Update `doc/plan.md` statuses as each phase completes and leave it empty only after all phases, review, and fixes are complete.

Edge cases: source tests that look for old helper names, dynamic diagnostic API compatibility inside Beryl-owned tools, memory-diagnostic field names consumed by diagnostic child tools, failure notices for residency loads, and user-visible strings that still imply transcript-local loading rows.

Verification: Run `cargo nextest run -p beryl-app` plus focused source searches for `Loading transcript details`, `TranscriptTurnSkeleton`, `skeleton`, `begin_transcript_turn_detail`, `TranscriptDetailLoad`, `released_history_placeholder`, and renderer/prepaint detail scheduling. Use a reviewer subagent for completion review because this work touches authoritative docs and code, then address any findings through this plan before clearing it.

Resumable milestone: All phases are finished, review findings are resolved, targeted tests pass, and `doc/plan.md` is emptied as the implementation plan archive convention.
