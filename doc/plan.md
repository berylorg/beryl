# Scope

Investigate Beryl-owned and dependency-owned code paths that can retain memory without a deterministic bound during ordinary GUI use.

This plan replaces the previous default-startup memory attribution plan. The starting evidence is that clean default populated-workspace startup is roughly 70 MiB Private Bytes, while post-task live sessions have been observed around 99 MiB and, during the earlier investigation, around 129 MiB. The best current hypothesis is retained live activity state plus warmed renderer, glyph, GPU, transcript, or backend-client caches.

The goal is to find high-impact retained structures that can grow without an explicit cap or defensible bound, classify their ownership, and propose concrete bounding strategies. The investigation should include Beryl Rust source, workspace crates, and the exact third-party dependency sources that own relevant retained state. It should avoid Windows-side FFI debugging and should not continue GUI-driven activity stress until Beryl exposes enough turn-state feedback for reliable automation.

This is an investigation and recommendation plan. Do not implement bounding changes until this plan is updated with explicit implementation phases and the operator has accepted the proposed direction. The AI may add more phases to this `doc/plan.md` as the investigation proceeds if new evidence shows that a separate focused audit, instrumentation pass, or implementation phase is needed.

Relevant design constraints:

- RAM efficiency and CPU efficiency are first-order design constraints.
- Hot-path buffers, queues, caches, and retained projections must have bounded or otherwise justified growth under normal operation.
- Large transcript scrolling and activity rendering must remain viewport-windowed.
- Activity presentation must render from an incrementally maintained in-memory projection and must not rebuild from transcript history during render.
- Background backend clients must be bounded, cancellable, and lower priority than foreground turn streaming and transcript activation.
- Backend-owned conversation history must remain backend-owned; Beryl's loaded transcript is a transient projection.

Edge-case checklist:

- Activity retention: verify whether activity records, derived rows, labels, subagent metadata, parent-child maps, and indexes have explicit caps or pruning tied to turn/thread/workspace lifecycle.
- Transcript projection: verify that loaded turn pages, rendered rows, Markdown caches, text layout caches, code panels, selection state, and media references are capped by page, row, byte, item, or lifecycle limits.
- Renderer and glyph caches: verify whether GPUI glyph atlases, image caches, text layout caches, and DirectX resources are bounded by size, window lifecycle, or dependency-owned eviction policy.
- Backend clients and async queues: verify that request maps, notification queues, streaming event buffers, background maintenance tasks, receiver slots, retry state, and cancellation paths cannot grow indefinitely after long sessions or errors.
- Workspace and graph state: verify that semantic graph projections, checklist projections, thread inventories, known-thread maps, title snapshots, and thread refs are bounded by durable workspace contents rather than repeated runtime events.
- Media and image assets: verify that decoded images, thumbnails, compressed bytes, pending composer images, transcript media cache entries, and failed-load records have deterministic memory caps and cleanup.
- Error and retry paths: check partial failures, turn errors, backend disconnects, stale metadata, cancelled turns, and workspace/thread switching for retained abandoned state.
- Multi-session/process safety: do not run another Beryl process against the active Beryl home; use copied homes for process experiments and exclude child `codex app-server` memory.
- Dependency notes: before expensive dependency exploration, verify exact versions and features, consult `doc/deps/`, and refresh dependency notes through read-only subagents when needed.

# Phase 1: Inventory retained state and caches (finished)

Build a source-level inventory of Beryl-owned retained state and caches that can grow with session length, backend event volume, transcript size, workspace size, media count, or background work.

The inventory should prioritize likely high-impact paths:

- `ToolActivityProjection` and activity-panel state.
- Conversation surface state, loaded transcript windows, pending input queues, and presentation projections.
- Markdown parsing/render caches, text-layout state, code-block panels, and selection geometry.
- Media caches, image assets, composer image payloads, and decoded raster handles.
- Backend client/session state, pending request maps, notification queues, streaming turn event handling, and background maintenance clients.
- Workspace projections, graph/checklist views, member-thread inventories, known-thread maps, and title metadata.
- Settings, notification, and window/preheated UI state only if inspection shows possible growth over time.

For each candidate, record the owner type/module, growth key, current cap or lack of cap, cleanup trigger, and likely relationship to the observed post-task live-session delta.

Verification cases:

- Confirm the audit distinguishes retained state from render-frame temporary allocations.
- Confirm each candidate is classified as explicitly bounded, naturally bounded by durable data, unbounded, or unknown.
- Confirm activity-state findings include both data retention and rendering-window behavior, because viewport-windowed rendering alone does not bound retained memory.

Phase 1 outcome, 2026-05-12:

- Four read-only source-audit slices inspected activity/transcript state, backend async state, workspace projections, and dependency-facing UI/media caches. No Beryl process was launched and no code was changed.
- Highest-priority unbounded live-session candidate: `ToolActivityProjection` in `crates/beryl-app/src/shell/tool_activity.rs`. It retains `records`, derived `rows`, agent labels, subagent runtime metadata, parent-child maps, visible-row indexes, and reasoning summary parts. Rendering is windowed, but retained activity is not capped by record count, byte count, thread count, or turn count. Cleanup is mainly surface/backend reset; finished activity can remain for the live workspace session.
- High-priority unbounded active-stream candidate: `ExecutionDetailState` and `TurnExecutionRecord` in `crates/beryl-app/src/shell/execution_detail.rs`. Historical resident pages are partially bounded by history release, but active/current turn text, reasoning content, command output, file-change output, narrative entries, and item count append without Beryl-side byte or item caps.
- High-priority queue/backlog candidates: `ManagedBackendSession::pending_messages`, `ShellView` worker receiver channels including `turn_receiver`, and `WorkspacePersistenceQueue`. These use unbounded buffering patterns around backend messages, stream updates, and full-state persistence commands. UI polling is time/count bounded, but producer-side memory is not bounded if the UI or persistence worker falls behind.
- High-priority media/image candidates: `TranscriptMediaCache` is LRU-bounded to 512 entries but not byte-bounded, retaining compressed bytes and `gpui::Image` handles. Composer draft image payloads have no explicit count or byte cap while active. Composer clipboard is capped by payload count, not bytes. Composer history is capped per lane but has no global lane cap, so image-bearing drafts can accumulate across many thread scopes.
- Medium-priority transcript projection candidates: `TranscriptPresentationState.rows` and `TranscriptHistoryWindow.pages` retain metadata/projection rows for loaded or visited history beyond the render-frame window. `TranscriptStreamProjection.entries` retains completed visible text projections until scope reset. `TranscriptMarkdownCache` is explicitly bounded by entry count and latest source bytes, but parsed/render structure overhead is not an exact byte budget.
- Medium-priority workspace/thread inventory candidates: `WorkspaceConversationState.threads`, `ConversationSurfaceState.known_threads`, `MemberThreadInventorySnapshot`, and `ThreadSelectorProjection` can grow with backend thread count and workspace activity. They are naturally related to durable or backend-owned sets, but there is no deterministic GUI-side count or byte budget.
- Medium-priority graph candidates: `GraphOverlayState` retains both committed and visible `SemanticGraph` aggregates, and `GraphMutationCoordinatorState` can retain queued commits and pending optimistic mutations while revisions are gapped or operations remain unresolved. These are durable-graph-size or lifecycle bounded, not deterministic memory bounded.
- Lower-priority but clear unbounded UI-state candidates: code-panel soft-wrap keys and resized heights can grow by distinct interacted code panels; `ComposerImageLabelState` can grow by observed thread count; `ToolActivityNicknameResolver` has bounded batch concurrency but an unbounded retry set; `TranscriptStreamInvalidations` can retain invalidated turn ids until reset.
- Explicitly or naturally bounded low-risk areas found in Phase 1 include visible transcript text geometry and selection frames, surface notices, settings state, one active image context-menu target, and one active image preview popup. These are viewport, fixed-count, or single-item bounded, though GPUI internals for images/text remain a Phase 3 dependency question.
- Phase 2 should start with `ToolActivityProjection`, active `ExecutionDetailState` stream buffers, worker/session queues, media/image byte caps, and composer image retention. Workspace inventories and graph duplication are important but more tied to durable data scale than to the observed single long-task live-session delta.

# Phase 2: Audit high-risk unbounded paths (finished)

Deeply inspect the candidates from Phase 1 that are unbounded, weakly bounded, or unknown, starting with activity retention and any structures retained after turn errors or long streaming turns.

For each high-risk path, determine:

- Whether growth is per event, per row, per turn, per thread, per workspace, per backend connection, per window, or per media item.
- Whether retained data contains large payloads, cloned strings, JSON/event objects, image bytes, layout objects, handles, or only compact metadata.
- Whether switching threads, opening a new workspace, cancelling a turn, interrupting a turn, backend disconnect, or turn error releases the state.
- Whether state can accumulate across multiple long tasks in the same GUI session without user-visible benefit.
- Whether Beryl already has enough semantic structure to prune safely without violating product behavior.

Suggested bounding strategies should be concrete, not just "add a cap". Examples include per-turn pruning, per-thread ring buffers, byte-budgeted caches, row-count windows with summary rows, lifecycle reset on terminal turn state, weak references for UI-only handles, LRU caches, and spill-to-durable metadata only when the design says the data is authoritative.

Verification cases:

- For every unbounded path, identify at least one deterministic bound candidate or explain why the data is intentionally bounded by an external durable set.
- Check failure and cancellation paths separately from normal successful turn completion.
- Check whether pruning would break transcript copy/quote behavior, activity panel semantics, subagent label resolution, graph/checklist provenance, or pending input retry behavior.

Phase 2 outcome, 2026-05-12:

- Five read-only source-audit slices inspected activity retention, execution/transcript projections, backend queues, media/composer state, and workspace/thread/graph state. No Beryl process was launched and no code was changed.
- `ToolActivityProjection` remains the clearest post-task live-session growth source. It retains completed activity records, derived rows, visibility indexes, labels, subagent runtime metadata, parent-child maps, and full reasoning-summary parts after success, error, cancellation, and thread switching. It resets on backend reopen/workspace replacement, but ordinary terminal turn states mark rows finished or failed without pruning. Activity data is presentation-only and non-authoritative, so the safe bound is to keep all running rows, keep a small number of completed groups per root thread, add a global row or byte budget, truncate display payloads at ingress, and prune related nickname/runtime maps when no retained row or active ownership references their thread ids.
- `ExecutionDetailState` and active `TurnExecutionRecord` payloads are high-impact retained transcript state. Agent text, reasoning summary/content, command output, file-change output, generated-image inline results, user fragments, backend user input records, and error text can grow per active turn and remain resident after successful completion or failure. History page release replaces cold turns with placeholders, but current and recently loaded turns keep payloads. Safe bounds need separate policies for transcript-authoritative visible text and non-transcript operational output: per-item and per-turn byte budgets for agent/user text, smaller budgets or terminal-state discard for raw reasoning content, head/tail truncation for command and file output, active generated-image inline result caps matching the existing 256 KiB history policy, and explicit behavior for over-budget copy/quote or reload.
- Transcript duplication is real even when render work is viewport-windowed. `TranscriptPresentationState.rows` duplicates projected turn content for displayed rows, `TranscriptStreamProjection.entries` duplicates completed visible markdown text until scope reset, and `TranscriptMarkdownCache` has entry/source caps but not exact parsed/render overhead caps. Recommended bounds are to retain stream entries only for active/current-frame keys, add byte accounting for parsed/render cache overhead where feasible, and cap released page metadata with pinned exceptions for the viewport, selection, edit/branch targets, active turn, and latest tail.
- Backend and worker queues have deterministic cleanup in many terminal paths, but they do not have deterministic producer-side bounds. `ManagedBackendSession::pending_messages`, stdio reader queues, `ShellView.turn_receiver`, active-turn steering tasks, pending input/steering queues, `WorkspacePersistenceQueue`, title-generation tasks, and member-thread inventory workers can retain arbitrary queued payloads while consumers lag or background work stalls. Recommended bounds are byte-budgeted bounded channels, event coalescing by thread/turn/item/kind for stream deltas, small protected caps for dynamic tool requests, bounded pending-fragment queues with user-visible rejection over cap, a coalesced persistence pending-state map instead of queued full-state clones, global title-worker concurrency caps, and inventory windows rather than fetch-all snapshots.
- Media and composer state is the main Beryl-owned large-byte surface. `TranscriptMediaCache` is LRU-bounded by entry count but not by compressed or decoded bytes. Transcript image menu/preview state is single-target but can retain one large image. Composer drafts, accepted draft history, and composer clipboard payloads can retain pasted image bytes; history is capped per lane but has no global lane or byte cap, and clipboard is capped by payload count rather than bytes. Recommended bounds are byte-budgeted media eviction, preview/menu byte caps, per-draft image count and total-byte limits, byte-aware clipboard eviction, global composer-history lane and byte budgets, and preferring durable image references over retained bytes once workspace assets are written.
- Workspace image assets are durable disk state rather than RAM, but their metadata and files can grow without garbage collection. `unreferenced_at_millis` is tracked, but no deletion path was found. This is outside the direct 99 MiB RAM question, but the same deterministic-resource principle suggests a later asset-GC phase with grace-period deletion and retained-reference checks.
- Workspace, inventory, selector, graph, and checklist projections are mostly durable-data-scale rather than likely single long-task deltas. Still, `WorkspaceConversationState.threads`, `known_threads`, `MemberThreadInventorySnapshot`, `ThreadSelectorProjection`, `GraphOverlayState`, `GraphMutationCoordinatorState`, `ChecklistSidebarProjectionCache`, and workspace-picker metadata have no deterministic in-process cap. Recommended bounds are pinned thread inventory windows, separating durable pinned thread refs from bounded metadata caches, clearing or age-capping closed thread-selector projections, graph mutation queue count/byte/time limits with full-reload recovery on revision gaps, overlay-patch representation or explicit graph size budgets, lazy/windowed checklist rows, and virtualized workspace picker/member-path loading.
- The main design-sensitive question left for recommendation work is which retained thread metadata is durable user state and which is cache. Pruning thread registry data without pinning rules could discard manual titles, bindings, rebind-required state, token snapshots, or graph thread refs. Phase 4 must separate cache-only evictions from changes that need a design decision.
- Phase 3 should focus on dependency-owned retained memory that Beryl cannot bound at the call site, especially GPUI image/glyph/text layout resources and `gpui-text-input` undo or buffer retention. Phase 4 should translate this Phase 2 audit into prioritized Beryl-owned bounding recommendations before implementation phases are proposed.

# Phase 3: Inspect relevant dependency caches and allocators (finished)

Inspect exact dependency versions for retained caches or resource pools that may grow during long sessions and are not directly visible as Beryl-owned collections.

Start with dependencies implicated by previous measurements or source ownership:

- `gpui` and Beryl's Zed fork for glyph atlases, text layout, image resources, window resources, and DirectX renderer caches.
- `gpui-text-input` for editor buffer, undo/redo, selection, shaping, and layout state used by composer and text fields.
- Markdown/rendering dependencies used by transcript presentation.
- WebSocket, async, and serialization dependencies used by backend streaming and request handling, if Phase 1 or 2 finds retained buffers in those paths.
- Image decoding or raster dependencies used by transcript media, if media cache growth is a candidate.

Follow the dependency-note contract: determine the exact resolved version and enabled features, consult matching `doc/deps/<crate>/<version>.md` notes, and use read-only subagents to create or refresh notes when the dependency exploration is expensive.

Verification cases:

- Confirm whether dependency caches have explicit item, byte, texture, atlas, or lifecycle limits.
- Confirm whether Beryl can configure those limits, must wrap usage with its own limits, or would need an upstream/fork change.
- Confirm that dependency-owned fixed startup cost is not mistaken for live-session unbounded growth.

Phase 3 outcome, 2026-05-12:

- Four dependency-source audit slices inspected GPUI, `gpui-text-input`, Markdown/image decoding, and backend protocol/runtime dependencies. No Beryl GUI process was launched and no code was changed. Dependency notes were refreshed or added for `gpui`, `gpui-text-input`, `markdown`, `image`, and `soketto`.
- The highest-impact dependency-owned retention remains GPUI image/resource handling. `gpui::App::loading_assets` caches completed asset tasks indefinitely unless the caller removes them; there is no item or byte cap. `gpui::Image::remove_asset` removes the asset-task entry, but already uploaded atlas tiles are not removed unless a `RenderImage` is dropped through the lower-level image-drop path. Beryl transcript media does not currently have an obvious explicit GPUI asset-removal lifecycle tied to `TranscriptMediaCache` eviction.
- GPUI decoded image retention can be large. `RenderImage` retains decoded BGRA frames, including all decoded frames for animated GIF/WebP inputs, without pixel, frame, or byte limits. Distinct `Image::from_bytes` payloads can therefore grow the GPUI app asset cache independently of Beryl's entry-count-bounded transcript media cache.
- GPUI's per-window sprite atlas has per-page dimensions but no total page or byte cap. Removed image tiles are not individually reusable while other keys on the same atlas page remain live. Glyph, SVG, and image atlas keys clear on window/device teardown, and image keys can be dropped through explicit image lifecycle, but there is no public cleanup path for glyph/SVG churn.
- GPUI text retention is split between bounded-frame and app-lifetime state. `LineLayoutCache` is current/previous-frame and should plateau for stable rendered text, but font resolution, font metrics, glyph raster bounds, wrapper-pool character width maps, and selected font maps are app-lifetime and unbounded by count or bytes. Beryl can mostly bound this indirectly by limiting font/size/glyph churn, not by configuring GPUI cache budgets.
- GPUI renderer buffers include fixed window resources and high-water structured buffers. Swapchain/path textures and pipeline buffers warm up with the first window and largest observed scene batches, then do not shrink until renderer drop or device-loss recreation. This can explain fixed startup/first-frame growth and should be separated from live-session unbounded growth in later diagnostics.
- `gpui-text-input` has no global cache, but every `TextInputState` retains current text plus undo and redo stacks of full-buffer `EditSnapshot`s. The default undo limit is `128` snapshots per stack, bounded by count but not bytes. Beryl currently disables undo only for the read-only surface-notice input; composer and settings inputs use the default unless wrapped differently.
- `gpui-text-input` also retains current widget layout in `last_layout`, bounded by current widget text and lifecycle but not by an explicit byte or line cap. Its shaping calls delegate to GPUI, so glyph/shaping cache growth belongs to the GPUI findings.
- `markdown 1.0.0` does not own long-lived parser caches in Beryl's `to_mdast` path. It builds temporary parser events and returns an owned mdast tree with owned strings. Beryl's own `TranscriptMarkdownCache` is the retention layer, currently entry/source-byte bounded but not exact parsed/render overhead bounded.
- `image 0.25.10` does not cache decoded rasters on Beryl's `load_from_memory_with_format` path. It does perform full transient decodes with default per-decode `max_alloc = 512 MiB` and no width/height limit. Beryl decodes once for dimensions, then GPUI decodes again for rendering; Beryl also retains compressed image bytes separately from `gpui::Image`'s encoded bytes.
- Runtime WebSocket transport uses `soketto`, while `tungstenite` is dev/test-only for Beryl. Beryl sets a 64 MiB runtime WebSocket frame/text-message cap, but `soketto::handshake::client::Client::handshake` grows a `BytesMut` in 8 KiB reads until the HTTP response parses complete with no deterministic byte cap. Already-read post-handshake bytes move into Beryl's `pending_read` queue.
- `serde_json` materializes whole values and strings for accepted payloads and provides no retained-memory bound beyond Beryl's input/message/queue budgets. The generated-image sanitizer's `IgnoredAny` skip path avoids retaining discarded JSON values, but accepted `Value` trees remain call-site bounded. `tokio-util::compat` does not add a meaningful retained queue. The inspected backend runtime path used `std::sync::mpsc::channel`, not a `tokio::sync` channel.
- Phase 4 recommendations should treat dependency-owned retention in two groups: fixed warm-up/high-water memory that may be acceptable but should be measured separately, and truly unbounded growth that Beryl should wrap or configure. The strongest wrapper candidates are GPUI image asset lifecycle, transcript media byte/frame limits, composer/settings text undo byte limits, image decode limits, and bounded WebSocket handshake/stdio/message queues.

# Phase 4: Produce bounded-memory recommendations (finished)

Write a concise findings document that lists high-impact unbounded or weakly bounded memory paths and recommended deterministic limits.

For each recommendation, include:

- The retained structure and owning module or dependency.
- The growth trigger and expected high-impact payload.
- The proposed bound, eviction or pruning policy, and lifecycle trigger.
- The user-visible behavior after pruning, including whether old activity becomes summarized, hidden, reloadable, or discarded.
- The implementation location and likely test coverage.
- Any design-doc change needed before implementation.

The report should separate immediate Beryl-owned fixes from dependency-owned limits that require configuration, wrapper policy, or GPUI fork changes.

Verification cases:

- Confirm every proposed bound preserves backend-owned transcript authority and does not synthesize or discard authoritative conversation history.
- Confirm any data discarded by Beryl is either presentation-only, reconstructable, or explicitly non-authoritative.
- Confirm recommendations are deterministic enough to state worst-case retained item counts or byte budgets.

Phase 4 outcome, 2026-05-12:

- Added `doc/memory-bounds-recommendations.md` with high-impact bounded-memory recommendations for activity projection, active turn details, backend/worker queues, transcript media and composer images, GPUI image lifecycle, transcript projections and Markdown cache, text-input undo state, workspace/inventory/graph/checklist projections, and protocol/dependency parsing limits.
- The recommendations separate immediate Beryl-owned caps from dependency-owned GPUI/image/protocol limits and from design-sensitive durable-data boundaries such as thread metadata, graph state, and checklist projections.
- Every recommended pruning policy is framed around preserving backend-owned transcript authority: discarded Beryl state is presentation-only, reconstructable, or explicitly non-authoritative unless a later accepted design change allows otherwise.
- Candidate deterministic budgets are documented for row counts, byte counts, image decode admission, queue lengths, pending graph mutations, text-input undo snapshots, and media/composer retention. These are recommendations only; no bounding implementation was performed in this phase.
- The report recommends adding diagnostics before behavior changes so retained activity bytes, active-turn bytes, media bytes, queue backlogs, GPUI asset counts where observable, and text-input undo bytes can be measured without unsafe GUI stress on the active Beryl process.

# Phase 5: Decide implementation phases (finished)

Update this plan with implementation phases only after the operator accepts the recommended bounds.

Verification cases:

- Confirm implementation phases are scoped to accepted recommendations.
- Confirm each implementation phase includes tests or reproducible diagnostics for the new bound.
- Confirm reviewer-subagent review is performed after all planned doc/code changes are complete.

Phase 5 outcome, 2026-05-12:

- The operator accepted `doc/memory-bounds-recommendations.md` as an implementation baseline.
- Updated `doc/design.md` to make deterministic bounds required for Beryl-owned runtime retention whenever externally variable input cannot be proven small by construction.
- Added implementation phases below. These phases should proceed one phase at a time. If a phase discovers that a recommended bound cannot work technically, stop and update this plan before trying an adapter or workaround.

# Phase 6: Add retained-memory diagnostics (finished)

Add diagnostics that expose counts and byte estimates for the retained structures targeted by this plan before changing behavior.

The diagnostics should cover at least:

- Activity projection records, rows, derived indexes, label strings, reasoning summaries, and subagent metadata.
- Active turn detail bytes, transcript-visible resident bytes, command/file-output bytes, reasoning bytes, generated-image inline bytes, and loaded transcript projection counts.
- Transcript media compressed bytes, estimated decoded image bytes, composer draft bytes, composer clipboard bytes, and accepted draft-history bytes.
- Backend/session queue lengths and byte estimates, persistence backlog state, pending input bytes, title/inventory worker counts, and dynamic tool request counts.
- GPUI image asset counts where Beryl can observe them, plus text-input undo snapshot counts and byte estimates.

Verification cases:

- Add tests or focused diagnostics fixtures for empty startup, default populated workspace startup, a synthetic long turn, and image-heavy state where practical.
- Confirm diagnostics do not scan full backend transcript history during render and do not require launching a second Beryl instance against the active home.
- Confirm diagnostic output distinguishes fixed warm-up or high-water dependency memory from Beryl-owned retained structures.

Phase 6 outcome, 2026-05-12:

- Added an app-neutral retained-count API to the sibling `gpui-text-input` crate: `TextInputRetainedCounts`, `TextInputState::retained_counts`, and `TextInput::retained_counts`. The API reports text, atom, undo, redo, and widget-layout lower-bound counts without Beryl concepts and without reaching into GPUI renderer internals.
- Extended Beryl memory milestones with a `retained_state = ?RetainedStateSnapshot` structured field containing detailed retained-count categories while preserving the existing individually logged fields. The snapshot now covers activity internals, active-turn payload categories, transcript presentation/history metadata, stream-projection text, media/Markdown cache stats, composer draft/clipboard/history bytes, pending input/steering queues, persistence backlog count, title/inventory worker counts, and text-input undo/redo diagnostics where a GPUI context is available.
- Added Beryl retained-count producers for `ToolActivityProjection`, `ExecutionDetailState`, transcript presentation/history, transcript stream projection, composer draft, composer clipboard, composer history, pending input queues, turn steering fragments, and workspace persistence backlog.
- Wired text-input diagnostics into the first transcript-render milestone after reset, where Beryl has the GPUI app context required to read `Entity<SingleLineInput>` values safely. Milestones produced outside a GPUI context still report Beryl-owned retained state without text-input internals.
- Existing transcript media and Markdown cache diagnostics remain the Beryl-observable GPUI/image proxy. True GPUI renderer, glyph, atlas, and uploaded image resource counts are not publicly observable without GPUI-fork instrumentation, so Phase 9 remains responsible for explicit GPUI image lifecycle limits.
- Beryl now reports worker receiver counts, backend client estimates, persistence backlog count, pending input bytes, pending steering bytes, title-worker count, and inventory-worker activity. Per-`ManagedBackendSession` private pending-message queues live inside worker-owned sessions and are not globally aggregatable from `ShellView` without the queue/channel redesign planned in Phase 10.
- Dynamic tool requests do not currently have a retained Shell queue: turn workers handle and respond to them inline. A retained dynamic-tool queue count is therefore not emitted in Phase 6; if Phase 10 introduces protected buffering, it must add diagnostics with that queue.
- Added focused tests for retained counts in activity, active-turn execution details, composer draft, composer clipboard, composer history, pending input queues, and stream projection. The `gpui-text-input` worker added tests for state and widget retained counts.
- Verification passed: `cargo fmt --check` in Beryl and `gpui-text-input`, `cargo check -p beryl-app`, and `cargo nextest run -p beryl-app` (991/991 tests). The `gpui-text-input` worker verified `cargo fmt --check` and `cargo nextest run` (29/29 tests). The only observed warning is the pre-existing GPUI `surface.rs` unreachable-code warning.

# Phase 7: Bound activity projection retention (finished)

Bound `ToolActivityProjection` while preserving active activity and selected-thread usefulness.

Implement the accepted policy from `doc/memory-bounds-recommendations.md`:

- Keep all running activity records.
- Retain completed activity by root turn with global row and display-byte budgets.
- Protect a latest completed-row window for the selected thread.
- Truncate large labels, arguments, and reasoning-summary display payloads at ingress.
- Prune nickname resolver state, subagent runtime metadata, parent-child maps, visible indexes, and other derived structures when no retained active or protected row references them.

Verification cases:

- Test success, error, cancellation, interruption, thread switching, workspace switching, and backend disconnect cleanup.
- Test parent-child/subagent hierarchy pruning without dangling rows or labels.
- Test that old completed activity is summarized or hidden without changing transcript content.
- Test that visible rendering remains viewport-windowed after retained history exceeds the cap.

Phase 7 outcome, 2026-05-12:

- Updated the `beryl-app` design contract so activity projection state is explicitly bounded in-memory session state: running activity is retained until terminal state, completed activity may be pruned by deterministic row, byte, and selected-thread windows, and ingress truncation remains presentation-only without mutating backend transcript history.
- Bounded `ToolActivityProjection` completed retention with local constants: 2,000 completed rows, 8 MiB completed display payload budget, 200 protected latest completed rows for the selected thread, 16 KiB label/display-value ingress limits, 64 KiB reasoning-summary retained payload limit, 64 reasoning-summary parts, and 64 retained receiver-thread ids per activity record.
- Preserved all running activity records and made selected-thread protection explicit by recording the current selected thread from `ConversationSurfaceState::apply_stream_event` before pruning decisions.
- Added pruning for activity records and derived structures: old completed rows are hidden when over budget, visible-row indexes are rebuilt from retained rows, and nickname labels, runtime metadata, parent-child links, root-turn links, and other derived maps are pruned when no retained running/protected activity path references them.
- Stored a bounded receiver-thread id list on retained collab-spawn records so retained parent activity can continue to own later-arriving child rows without keeping unrelated child metadata forever.
- Added tests covering global row-budget pruning, display-byte-budget pruning, selected-thread protected windows, retained spawn ownership before child rows arrive, subagent metadata/map pruning after rows age out, ingress truncation, terminal failure/declined status handling through existing fixtures, and visible row windowing after retained history exceeds the cap.
- Verification passed: `cargo fmt --check`, `cargo check -p beryl-app`, `cargo nextest run -p beryl-app --test tool_activity` (46/46 tests), `cargo nextest run -p beryl-app` (997/997 tests), and `git diff --check`. The only observed warning remains the pre-existing GPUI `surface.rs` unreachable-code warning, plus expected test-module dead-code warnings during the full suite.
- No Beryl GUI process was launched, and `beryl-standalone.exe` was not touched.

# Phase 8: Bound active-turn and transcript projection retention (finished)

Bound active `ExecutionDetailState`, `TurnExecutionRecord`, transcript projections, and Markdown cache overhead without making Beryl authoritative for backend transcript history.

Implement the accepted policy from `doc/memory-bounds-recommendations.md`:

- Split transcript-visible resident text from non-authoritative operational detail.
- Enforce per-item and per-turn byte budgets for live transcript text retained by Beryl.
- Enforce smaller budgets or terminal-state discard for raw reasoning detail, command output, file-change output, generated-image inline payloads, backend user fragments, and error text.
- Retain over-budget identity, byte counts, and reload markers so copy, quote, and display do not present truncated text as exact full text.
- Pin the viewport, selection, edit/branch targets, active turn, and latest tail while releasing older loaded-page metadata and stream projections to placeholders.
- Budget Markdown cache retention by source bytes and estimated parsed/render bytes.

Verification cases:

- Test successful completion, turn error, cancellation, interruption, backend disconnect, cold-page reload, and thread switch.
- Test copy and quote behavior over exact loaded text, truncated operational detail, and reloadable transcript-visible text.
- Test selection, edit, branch, active-turn, and latest-tail pins.
- Test large Markdown blocks and generated-image inline payload caps.

Phase 8 outcome, 2026-05-12:

- Updated the `beryl-app` transcript design contract to state that resident historical pages, released-page metadata, stream projection entries, and Markdown parse/render caches are bounded runtime projections, while backend-owned conversation history remains authoritative.
- Bounded active-turn operational detail in `ExecutionDetailState`: raw reasoning detail is capped while live and cleared on terminal turn states, command output and file-change output are capped with explicit omission markers, backend error detail is capped, and live generated-image inline result bytes now follow the same 256 KiB admission cap as historical generated-image results. Saved generated-image paths continue to take precedence over retained inline bytes.
- Kept loaded transcript-visible agent/user narrative exact rather than truncating it locally. Exact loaded text continues to power copy, quote, edit, and branch behavior. Older loaded turns are bounded through the existing page-release path, which replaces them with honest backend-identity placeholders rather than pretending truncated text is exact.
- Bounded released transcript-history metadata with a deterministic 32 released-page cap while retaining the existing 4 resident-page cap and latest-page pin.
- Bounded `TranscriptStreamProjection` completed entries with deterministic caps of 512 completed entries and 2 MiB completed visible text, while preserving active uncommitted entries. Completed projection text is discardable because exact text is retained by loaded turn records or reloadable from backend history.
- Added estimated retained-byte accounting and eviction to `TranscriptMarkdownCache`. The cache remains entry-count and source-byte bounded, and now also evicts by conservative estimated retained bytes covering duplicate source strings plus parsed/render structure overhead. Diagnostics now report estimated retained bytes, in-flight source bytes, displayed source bytes, parsed source bytes, and estimated structure bytes.
- Retained-memory diagnostics now add Markdown estimated retained bytes and stream-projection retained counts into the logged lower-bound snapshot.
- Added focused tests for operational stream payload caps, turn-error caps, live generated-image inline caps, released-page metadata caps, stream-projection completed-entry pruning, active uncommitted stream-entry preservation, Markdown retained-byte diagnostics, and Markdown estimated-byte eviction. Existing transcript release, selection, edit/branch, interruption/failure, reset, and thread-switch tests continue to cover the unchanged exact-loaded-text and placeholder honesty behavior.
- Verification passed: `cargo fmt --check`, `cargo check -p beryl-app`, `cargo nextest run -p beryl-app --test conversation_execution --test transcript_history --test transcript_stream_projection --test transcript_markdown_cache` (60/60 tests), `cargo nextest run -p beryl-app` (1005/1005 tests), and `git diff --check`. The only observed warnings remain the pre-existing GPUI `surface.rs` unreachable-code warning plus expected test-module dead-code warnings.

# Phase 9: Bound media, composer images, and GPUI image lifecycle (finished)

Bound Beryl-owned image bytes first, then add dependency-facing release hooks where GPUI exposes enough lifecycle control.

Implement the accepted policy from `doc/memory-bounds-recommendations.md`:

- Add byte-budgeted eviction for `TranscriptMediaCache`, including compressed bytes and estimated decoded bytes.
- Add per-draft image count and byte limits, composer clipboard byte eviction, and global accepted draft-history byte budgets.
- Prefer durable workspace image-asset references over retained byte copies after accepted paste persistence.
- Add explicit release calls for Beryl-created `gpui::Image` or GPUI asset handles when transcript media, composer previews, or popups are evicted.
- Add image decode admission limits before Beryl or GPUI accepts large or animated inputs.
- If GPUI cannot release uploaded atlas or decoded image resources deterministically, plan a separate GPUI-fork patch rather than relying on hidden lifecycle behavior.

Verification cases:

- Test many transcript images, large pasted images, animated images, missing files, path-rejected images, preview popup close, media cache churn, thread switching, and workspace switching.
- Test over-budget paste rejection before draft mutation.
- Test accepted paste persistence and history eviction without losing durable workspace image assets.
- Test that image eviction leaves reloadable placeholders or honest unavailable fallbacks, not stale handles.

Phase 9 outcome, 2026-05-12:

- Updated the root and `beryl-app` design contracts so transcript media, composer image payloads, clipboard/history image retention, and image fallbacks have deterministic Beryl-owned memory bounds.
- Added transcript media admission limits before full decode: 64 MiB compressed input, 32 megapixels, and 128 MiB estimated decoded bytes. Oversized media now renders an honest `(image too large)` fallback instead of constructing a GPUI image.
- Added byte-budgeted `TranscriptMediaCache` eviction in addition to the existing entry cap: 64 MiB compressed loaded image bytes and 128 MiB estimated decoded image bytes. Cache lookups and stale completions now return evicted `gpui::Image` handles for explicit release.
- Added GPUI image release calls for transcript media cache eviction, media-cache clear on scope/reset, transcript preview popup close, and composer image preview popup replacement/close/removal. Composer preview popups now cache one `Arc<Image>` for the popup instead of recreating a GPUI image during render.
- Bounded active composer drafts to 20 images and 64 MiB retained image bytes, rejecting over-budget paste before mutating the draft where the runtime path can validate first.
- Bounded composer clipboard image retention with byte-aware eviction, and bounded accepted composer-history retention with a global lane cap plus a 64 MiB image-byte budget.
- Changed accepted composer history to prefer durable workspace image-asset references without retaining byte copies after asset persistence, while preserving backend delivery through durable asset ids and paths.
- Documented the remaining GPUI residual in `doc/deps/gpui/0.2.2.md`: `Image::remove_asset` clears GPUI's loading-asset cache, but deterministic uploaded atlas/page memory reclamation still requires a GPUI-fork lifecycle or atlas-budget change if strict GPU-side byte bounds become required.
- Verification passed: `cargo fmt --check`, `cargo check -p beryl-app`, `cargo nextest run -p beryl-app --test composer_draft --test composer_clipboard --test composer_history --test transcript_media_sources --test composer_image_delivery --test composer_submission` (62/62 tests), `cargo nextest run -p beryl-app --test transcript_image_preview` (2/2 tests), `cargo nextest run -p beryl-app` (1013/1013 tests), and `git diff --check`. The only observed warning remains the pre-existing GPUI `surface.rs` unreachable-code warning plus expected test-module dead-code warnings.
- No Beryl GUI process was launched, and `beryl-standalone.exe` was not touched.

# Phase 10: Bound backend, worker, persistence, and protocol queues (finished)

Replace unbounded producer-side buffering with bounded queues, coalescing, and visible rejection where foreground correctness requires it.

Implement the accepted policy from `doc/memory-bounds-recommendations.md`:

- Bound backend stream event queues by count and byte budgets per active thread after coalescing.
- Bound protected dynamic tool request queues and pending input text/image queues.
- Coalesce persistence work to one pending state per workspace instead of queueing full-state clones.
- Bound or cancel stale title-generation and inventory-refresh work with explicit concurrency limits.
- Add bounded protocol input paths for WebSocket handshake response bytes, stdio line/message buffers, accepted JSON payloads, and image dimension/decode readers.

Verification cases:

- Test UI consumer stalls, backend disconnect, turn error, cancellation, interruption, workspace switch, and app shutdown.
- Test foreground turn streaming remains protected while stale background title or inventory work is dropped or refreshed later.
- Test over-budget pending input rejection before enqueue.
- Test oversized handshake, JSON, stdio output, and image inputs fail with bounded-resource errors.

Phase 10 outcome, 2026-05-12:

- Updated the `beryl-app` and `beryl-backend` design contracts for bounded backend/session queues, pending input queues, turn-stream delivery, coalesced persistence, bounded title generation, bounded member-thread inventory windows, and bounded protocol input paths.
- Bounded backend client deferred-message retention with a 1,024-message cap, an approximate 16 MiB retained-byte budget, and a protected 64 dynamic-tool-request cap. Exceeding these limits now returns a bounded-resource error instead of retaining an unbounded notification or server-request backlog.
- Bounded WebSocket connection setup with a 10-second handshake timeout and a 4 KiB handshake read-ahead cap. Bounded stdio transport producer paths with a 64-message sync channel plus stdout/stderr line caps, and oversized or invalid protocol input no longer retains rejected full line payloads.
- Bounded app pending input retention: pending next-turn input and pending active-turn steering queues enforce 64-fragment and 1 MiB retained-payload budgets. Over-budget input is rejected before transcript/execution queue mutation and reports an `Input queue full` surface notice.
- Bounded active-turn steering and foreground turn-stream delivery. Steering workers are capped at four concurrent tasks, overflow falls back to the bounded next-turn queue, and turn-worker updates use a bounded sync channel so a stalled UI cannot accumulate an unbounded stream backlog.
- Replaced unbounded workspace-persistence command buffering with a coalescing worker queue. Repeated workspace state, UI state, token snapshot, and image-asset mark commands merge by durable target while preserving flush boundaries.
- Bounded background title and inventory work. Automatic title generation keeps at most four workers alive and truncates retained prompt seed text, while member-thread inventory snapshots keep the newest 2,048 matching backend thread rows before metadata enrichment.
- Added or updated focused tests for pending input overflow, turn-worker consumer rejection, persistence coalescing and flush boundaries, title seed truncation, inventory truncation, backend pending-message count and byte overflow, dynamic-tool overflow, and WebSocket handshake read-ahead overflow. Stdio stdout/stderr line caps are implemented in the transport reader path; a dedicated stdio process-fixture overflow test was not added because the managed stdio launch path invokes the real `codex app-server`.
- Verification passed: `cargo fmt --check`, `cargo check -p beryl-app -p beryl-backend`, `cargo nextest run -p beryl-app` (1022/1022 tests), `cargo nextest run -p beryl-backend` (118/118 tests), and `git diff --check`. The only observed warning remains the pre-existing GPUI `surface.rs` unreachable-code warning plus existing test-module dead-code warnings.
- No Beryl GUI process was launched, no second Beryl process was run against the active Beryl home, and `beryl-standalone.exe` was not touched.

# Phase 11: Bound text-input undo and small UI caches (finished)

Bound count-only or lifecycle-only GUI convenience state that is unlikely to explain the largest memory deltas alone but violates the accepted deterministic-growth rule.

Implement the accepted policy from `doc/memory-bounds-recommendations.md`:

- Add byte-aware undo and redo limits for composer and settings `gpui-text-input` states.
- Clear or compact undo state after successful submit and after accepted pending input has durable image references.
- Bound code-panel soft-wrap and resized-height maps by retained transcript/page identity.
- Bound composer image label state, tool-activity nickname retry state, transcript invalidation sets, and similar retry or interaction caches.

Verification cases:

- Test large paste, repeated edits, undo/redo cap behavior, submit, draft restore, and settings-field editing.
- Test transcript page release also releases matching code-panel interaction state.
- Test retry/invalidation caps across thread switch, workspace switch, turn error, and backend disconnect.

Phase 11 dependency integration decision, 2026-05-12:

- Reconnaissance found that clean byte-aware undo/redo limiting belongs in the standalone `gpui-text-input` public API, with `gpui-settings-window` exposing app-neutral settings-field configuration on top of it. That matches the architectural requirement to keep Beryl-specific behavior out of both dependency crates.
- Beryl consumes `gpui-text-input` and `gpui-settings-window` by pinned remote git revisions in the root `Cargo.toml`. The operator approved using ignored local `.cargo/config.toml` patch entries during implementation and testing, while keeping committed dependency declarations as pinned GitHub revisions.
- The final plan step must replace the old pinned GitHub revisions with correct new revisions after the sibling dependency changes are committed locally. Until those commits are pushed and fetchable from GitHub, local validation depends on the approved `.cargo/config.toml` patches and clean remote-only validation remains deferred.

Phase 11 outcome, 2026-05-12:

- Updated the standalone `gpui-text-input` crate with app-neutral byte-aware undo/redo retention. `TextInputOptions` now has a per-stack undo byte budget, snapshot insertion enforces both count and byte budgets, over-budget snapshots are not retained, and `TextInputState`/`TextInput` expose `clear_edit_history` without changing current text.
- Updated the standalone `gpui-settings-window` crate with an app-neutral `SettingsWindowOptions::with_text_input_undo_byte_limit` setting. Settings row inputs, color-picker field input, and color-picker channel inputs now construct `gpui-text-input` fields with that budget.
- Wired Beryl direct inputs to explicit undo byte budgets: small single-line startup/picker/rename inputs use a small budget, the composer uses a larger bounded budget, settings fields receive a Beryl settings-window budget, and the composer clears edit history at the central draft-clear path.
- Bounded small Beryl UI caches: code-panel soft-wrap and resized-height state is pruned on transcript row release/reset with a retained-panel cap; composer image label thread metadata is capped and scan failure text is truncated; pending-new-thread label-scope bindings are capped; tool-activity nickname retries are pruned to retained resolution targets with a global cap; transcript stream invalidations are capped by thread count, per-thread turn count, and total turn count.
- Updated design docs for `gpui-text-input`, `gpui-settings-window`, and `beryl-app`, and refreshed the `gpui-text-input` dependency note.
- Verification passed under the approved local `.cargo/config.toml` patches: `cargo nextest run` in `gpui-text-input` (35/35), `cargo nextest run` in `gpui-settings-window` (21/21), focused `beryl-app` tests for composer image labels, nickname retry, stream invalidation, and Markdown code-panel ids (26/26), full `cargo nextest run -p beryl-app` (1029/1029), `cargo check -p beryl-app -p beryl-backend`, `cargo fmt --check` in all three affected projects, and `git diff --check` in all three affected projects. The only compiler warning observed remains the pre-existing GPUI `surface.rs` unreachable-expression warning plus existing test-module dead-code warnings.

# Phase 12: Bound workspace inventory, graph, and checklist projections (finished)

Define durable-state versus cache boundaries before pruning workspace-scale projections, then bound presentation-only metadata.

Implementation must preserve exact durable GUI-owned state, including manual thread titles, member bindings, rebind-needed state, last-known token snapshots when design says they are durable, graph thread refs, semantic graph records, checklist records, and provenance.

Implement the accepted policy from `doc/memory-bounds-recommendations.md` only after the durable/cache split is explicit:

- Window member-thread inventory snapshots and thread selector projections.
- Age-cap closed selector projections and virtualize selector/checklist rows.
- Cap graph mutation queues by count, bytes, and elapsed time, with full-reload recovery for revision gaps.
- Keep graph/checklist durable state exact while bounding derived overlay or presentation metadata.
- Add a later asset-garbage-collection plan if unreferenced durable workspace image files need disk-space bounds.

Verification cases:

- Test manual-title preservation, member binding preservation, rebind-needed state, token snapshot behavior, graph refs, provenance, and checklist item identity.
- Test inventory refresh after stale window eviction and selector reopen.
- Test graph mutation failure, revision gaps, cancellation, workspace switch, and full-reload recovery.
- Test checklist expansion and collapse across large graphs without retaining all derived rows.

Phase 12 outcome, 2026-05-12:

- Updated the `beryl-app` design contract to make the durable/cache boundary explicit for selector state, graph mutation queues, `known_threads`, and checklist sidebar projection. Durable workspace registrations, manual titles, member bindings, rebind-needed state, token snapshots, semantic graph records, checklist records, thread refs, and provenance remain exact durable state.
- Bounded reusable column-selector presentation state with explicit clear helpers and a retained expansion-override cap. Thread selector close now drops its derived projection, columns, and scroll handles; reopening rebuilds from the latest bounded member-thread inventory snapshot.
- Kept the existing member-thread inventory 2,048-row window and added a bounded selected-surface `known_threads` cache policy: backend-supplied display strings are truncated, the retained list is capped by count and byte budget, and selected-thread identity is pinned while durable workspace thread metadata remains untouched.
- Changed checklist sidebar projection to retain only selected-checklist metadata, row count, and a content fingerprint. Visible rows are materialized from the current semantic graph for the render window, preserving item identity, numbering, order, status labels, and thread-start affordances without retaining a full row-payload vector.
- Bounded graph mutation coordinator runtime queues by count, retained bytes, and elapsed time. If gapped commits or pending optimistic mutations exceed budget, the shell clears runtime projection queues, reports recovery status, and reloads exact graph state from workspace persistence off the `gpui` thread.
- Added focused tests for column-selector expansion/scroll cleanup, thread-selector close/reopen projection eviction, checklist row materialization from current graph, gapped commit count and byte recovery, and pending optimistic mutation pruning. Existing workspace-conversation and graph persistence tests continue to cover durable manual title, rebind, token snapshot, graph ref, and provenance preservation.
- Verification passed: `cargo check -p beryl-app`, focused `cargo nextest run -p beryl-app --test column_selector --test thread_selector --test checklist_sidebar_projection --test graph_overlay --test member_thread_inventory --test workspace_conversation_state` (144/144), full `cargo nextest run -p beryl-app` (1035/1035), `cargo check -p beryl-app -p beryl-backend`, `cargo fmt --check`, and `git diff --check`. The only observed compiler warning remains the pre-existing GPUI `surface.rs` unreachable-expression warning plus expected test-module dead-code warnings.
- No Beryl GUI process was launched, no second Beryl process was run against the active Beryl home, and `beryl-standalone.exe` was not touched.

# Phase 13: Review, validation, and closure (wip)

Run focused validation after the implementation phases are complete.

Required validation:

- Run the relevant `cargo-nextest` suites and any targeted diagnostics added in Phase 6.
- After the sibling `gpui-text-input` and `gpui-settings-window` changes are committed locally, update Beryl's pinned dependency revisions in `Cargo.toml` to the intended final commit ids. Local `.cargo/config.toml` patches may remain in use for validation until those commits are pushed as the final operator-controlled action.
- After the final push makes the sibling commit ids fetchable from GitHub, perform clean remote-only dependency validation and update `Cargo.lock` if Cargo records additional source metadata without local patches.
- Capture retained-structure diagnostics for clean default startup and at least one synthetic long-turn or image-heavy scenario.
- Confirm documented bounds are enforced by tests or diagnostics rather than only by comments.
- Use a reviewer subagent to review all authoritative doc and code changes, then plan and address any required fixes before closing this plan.
- Empty `doc/plan.md` only after all phases are complete and reviewer findings are resolved.

Verification cases:

- Confirm no implementation phase contradicted `doc/design.md`.
- Confirm no second Beryl process was run against the active Beryl home.
- Confirm all retained structures identified in `doc/memory-bounds-recommendations.md` are either bounded, explicitly deferred with an accepted reason, or moved into a new plan.

Phase 13 dependency-pinning update, 2026-05-12:

- Earlier in this phase, the dependency-pinning requirement was blocked because the sibling Phase 11 dependency edits were working-tree-only in `../gpui-text-input` and `../gpui-settings-window`.
- The operator clarified that Beryl should place the final intended commit ids as `rev` values now, while keeping local `.cargo/config.toml` patches for validation until pushing the sibling commits as the last action.
- This unblocks Beryl manifest pinning before remote availability, but clean remote-only dependency validation remains deferred until after those sibling commits are pushed.

Phase 13 sibling-commit readiness work, 2026-05-12:

- Done: committed the reviewed `gpui-text-input` undo-byte-limit API changes locally as `fd4f2caf8c39981f7b829f276c7ade48430eca83` (`Bound text input undo history by bytes`).
- Done: fixed `gpui-settings-window` so its committed dependency metadata points at `gpui-text-input` commit `fd4f2caf8c39981f7b829f276c7ade48430eca83`.
- Done: fixed `gpui-settings-window` option synchronization so theme or saved-swatches updates do not reload field text from the model and discard unsynchronized user edits, and undo-limit updates rebuild inputs from their current draft text.
- Done: validation passed for `gpui-settings-window` with 22/22 `cargo nextest run` tests, `cargo fmt --check`, and `git diff --check`; a reviewer subagent re-reviewed the previous blockers and reported no blocker findings.
- Done: committed `gpui-settings-window` locally as `aca63bffab3ce9e2db45c974f11d9f3e07123ae1` (`Configure settings input undo byte limits`).
- Done: updated Beryl's root `Cargo.toml` to pin `gpui-text-input` to `fd4f2caf8c39981f7b829f276c7ade48430eca83` and `gpui-settings-window` to `aca63bffab3ce9e2db45c974f11d9f3e07123ae1`.
- Remaining Phase 13 dependency-pinning constraint: these commits are local only until pushed. Local `.cargo/config.toml` patches keep validation working before push, but clean remote-only validation of those GitHub revs must wait until the final push step makes both commits fetchable from their remotes.

Phase 13 reviewer-finding fix plan, 2026-05-12:

- Fix active-turn steering fallback so moving an accepted fragment from active-turn steering into the pending-turn queue is transactional. If pending-turn admission would exceed its deterministic count or byte budget, the fragment must remain visible in active-turn steering state and must not be silently dropped.
- Add a regression test for over-budget steering fallback that proves the fragment remains in active-turn state when pending-turn queue admission rejects it.
- Fix transcript image preview replacement so opening a new loaded preview releases the previous loaded preview image through the same GPUI image lifecycle hook used by the close path.
- Add or update a focused test for preview replacement release behavior if existing seams make it practical without launching the GUI.
- Refresh dependency notes that name `gpui-text-input` and `gpui-settings-window` helper revisions so they match the new pinned commits in `Cargo.toml`.
- Resolve Phase 13 validation bookkeeping after code fixes: record the successful non-GUI suites, the reviewer findings and fixes, the clean-startup diagnostic attempt/result, and the remaining remote-only validation/livetest gates that require final push or operator live testing.

Phase 13 reviewer-finding fixes, 2026-05-13:

- Done: made active-turn steering fallback transactional before mutating `ExecutionDetailState`. Beryl now converts the candidate steering fragments to pending-turn fragments, validates the whole batch against the pending-turn count and byte budgets, queues the validated fragments, and only then removes the steering fragments from the active-turn record. Over-budget fallback reports `Input queue full` while leaving the visible accepted steering input intact.
- Done: added a pending-turn regression test proving batch admission rejects over-budget steering fallback without mutating the existing pending queue, and rejects wrong-thread fallback.
- Done: fixed transcript image preview replacement to close the previous popup before installing a new loaded preview, so the existing close-path GPUI image release hook runs for replacement as well as explicit close. A direct replacement unit test was not practical without launching the GUI panel; existing preview-close tests plus the centralized close-path call cover the lifecycle path.
- Done: refreshed dependency notes for the pinned helper commits. `doc/deps/gpui/0.2.2.md` and `doc/deps/gpui-text-input/0.1.0.md` now name `gpui-text-input` commit `fd4f2caf8c39981f7b829f276c7ade48430eca83` and `gpui-settings-window` commit `aca63bffab3ce9e2db45c974f11d9f3e07123ae1`.

Phase 13 validation status, 2026-05-13:

- Passed after reviewer-finding fixes: `cargo fmt --check`.
- Passed after reviewer-finding fixes: focused `cargo nextest run -p beryl-app --test pending_turn_input --test transcript_image_preview --test conversation_execution --test turn_worker` (61/61 tests).
- Passed after reviewer-finding fixes: `cargo check -p beryl-app -p beryl-backend`.
- Passed after reviewer-finding fixes: full `cargo nextest run -p beryl-app -p beryl-backend` (1154/1154 tests).
- Passed after reviewer-finding fixes: `git diff --check`.
- Passed for diagnostic-focused non-GUI suites: `cargo nextest run -p beryl-app --test memory_diagnostics --test transcript_presentation_large --test transcript_markdown_cache --test composer_draft` (31/31 tests).
- Passed: release build for the diagnostic binary path, `cargo build --release -p beryl`.
- Captured one sacrificial-home startup diagnostic run at `target/memory-diagnostics/20260513-000031/stdout.log` with `--memory-milestones` and `--beryl-home-dir target/memory-diagnostics/20260513-000031/home`. This run used a copied/sacrificial home rather than the active Beryl home and did not touch `beryl-standalone.exe`.
- Diagnostic milestones captured included `app_startup`, `app_state_resolved`, `gpui_application_created`, `main_window_opened`, `workspace_open_start`, `workspace_open_worker_start`, `workspace_persistence_flush_done`, `workspace_metadata_load_start`, `workspace_metadata_load_done`, and `backend_launch_start`. The latest captured milestone before stopping was around 69.7 MB Private Bytes and 47.0 MB Working Set. This run did not reach a full transcript-render milestone before stop.

Phase 13 reviewer re-review outcome, 2026-05-13:

- Done: reviewer re-review found the previous findings resolved and reported no new blocker findings.
- Verified by reviewer: steering fallback validates pending-turn admission before mutating active-turn state; transcript image preview replacement closes the previous popup and uses the existing GPUI image release path; dependency notes match the pinned helper commits; Phase 13 remains `wip` with remote-only validation and live-testing gates still explicit.
- Reviewer commands: targeted source reads and searches, `cargo nextest run -p beryl-app --test pending_turn_input --test transcript_image_preview` (10/10 tests), and `git diff --check`.
- Residual risk recorded by reviewer: preview replacement release is verified statically through the centralized close path rather than by a direct popup-replacement lifecycle test, and clean remote-only dependency validation plus live retained-structure diagnostics remain Phase 13 gates.

Remaining Phase 13 gates:

- Operator live testing is still needed before final commit/push readiness.
- Clean remote-only dependency validation remains deferred until the sibling helper commits are pushed and the Beryl `rev` pins are fetchable from GitHub without local `.cargo/config.toml` patches.
- `doc/plan.md` must remain non-empty until those gates are complete, final reviewer findings are resolved, and the final closeout commit/push path is explicit.
