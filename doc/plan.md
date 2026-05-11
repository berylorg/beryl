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

# Phase 5: Decide implementation phases (wip)

Update this plan with implementation phases only after the operator accepts the recommended bounds.

Phase 5 status, 2026-05-12:

- Blocked pending operator acceptance of `doc/memory-bounds-recommendations.md` as the implementation baseline, or a list of bounds and priorities to change before implementation planning.
- Do not add implementation phases until that acceptance or adjustment is provided.

Potential implementation phases may include:

- Add diagnostics that expose retained activity, transcript, media, cache, and backend-client counts and byte estimates.
- Bound `ToolActivityProjection` retained records and derived indexes.
- Bound transcript/media/text caches by byte and item budgets.
- Add lifecycle cleanup for turn-error, cancellation, backend disconnect, thread switch, workspace switch, and window close paths.
- Configure or patch dependency-owned caches when Beryl cannot bound them at the call site.

Verification cases:

- Confirm implementation phases are scoped to accepted recommendations.
- Confirm each implementation phase includes tests or reproducible diagnostics for the new bound.
- Confirm reviewer-subagent review is performed after all planned doc/code changes are complete.
