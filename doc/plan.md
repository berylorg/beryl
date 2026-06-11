# Scope

Implement the transcript scroll/render responsiveness architecture described by `doc/features/transcript/design.md`, `doc/ui.md`, `doc/features/diagnostics/design.md`, and `crates/beryl-app/doc/design.md`.

The implementation must keep transcript residency, render snapshots, media admission, speculative media preload, row presentation, code-panel state, scrollbar invalidation, and diagnostics on clean ownership boundaries. Ordinary transcript render, prepaint, scroll, scrollbar, status-line, and frame-metric paths must not scan resident or indexed history, parse offscreen Markdown, schedule resident history loads, initiate completed-media readiness for visible rows, or perform broad retained-state diagnostics.

The starting point is the current resident-window transcript architecture. Preserve its user-visible behavior: existing-thread activation publishes coherent presentable resident content atomically, scrolling clamps at nonresident or media-pending boundaries, loaded turns extend on demand only after their rows are presentable, released turns do not render skeleton or loading rows, semantic scroll anchors are preserved, and transcript selection/quote/context-menu behavior remains correct for resident content.

Readiness:

- Design authority has been updated before implementation.
- No migration adapters or compatibility branches for old skeleton/detail loading are allowed.
- If a planned phase turns out technically invalid, stop and report the contradiction instead of inventing a workaround.

Resumable milestone:

- Continue Phase 7 by introducing the explicit row or row-block presentability state, then gate selected-thread activation and scroll-boundary history extension before `ConversationSurfaceState::load_thread_history_window` and `ConversationSurfaceState::finish_loading_thread_history_page` publish rows.
- Keep this file current after each phase.
- When a phase is finished and later phases remain, stop according to the project continuation policy.

# Phase 1: Incremental Residency Diagnostics And Full Frame Profiling (finished)

Hard task:

Replace render-time residency retained-count scans with incrementally maintained transcript residency/frame counters, and expand transcript frame profiling so snapshot construction and render-adjacent pruning are measured.

Implementation notes:

- Add a residency stats structure owned by the transcript residency/history state.
- Update stats at every mutation point: index insertion, full-detail admission, release, pinning, unpinning, policy changes, page request start, page request finish, page request failure, and budget-reason changes.
- Make transcript panel snapshot construction consume copied stats only.
- Keep explicit retained-state summary diagnostics available, but route them through diagnostic-tool paths rather than render paths.
- Start transcript frame timing early enough to include snapshot construction and render-adjacent state pruning.

Edge cases:

- Stale or failed page loads must decrement in-flight counters exactly once.
- Replacing a resident page with released slots must subtract resident bytes and item counts exactly once.
- Pinned active turns and menu/edit/media targets must remain counted as pinned even when outside viewport priority.
- Thread activation, thread reset, workspace change, and backend-unavailable transitions must clear or replace stats atomically with the surface state.
- Diagnostic tools must not mutate counters while computing explicit retained summaries.

Verification:

- Unit coverage for stats deltas across admission, release, pin, unpin, reset, failed worker, and policy change.
- Source guard proving transcript snapshot construction does not call full retained-count scans.
- Existing transcript history and diagnostic tests.
- Live `read_transcript_frame_metrics` sample showing residency fields still populate without whole-index scans.

# Phase 2: Budgeted Transcript Media Preload Coordinator (finished)

Hard task:

Move transcript media preload out of prepaint into a budgeted coordinator that consumes viewport facts, coalesces requested ranges, and performs Markdown media-run planning and media requests within explicit row, time, byte, and in-flight limits.

Implementation notes:

- Prepaint reports visible range, desired preload range, viewport height, row identities, and layout facts only.
- The coordinator owns scheduling, cancellation, stale-result rejection, request coalescing, media-run planning, decode/upload admission, and diagnostics.
- Cache Markdown media-run segmentation by stable Markdown key plus source revision.
- Keep visible rows coherent when preload is absent or delayed.

Edge cases:

- Rapid scroll direction changes must cancel or supersede stale preload work.
- Released rows must drop pending preload results before they mutate visible media state.
- Generated-image saved-path media and Markdown local-image media must preserve their existing path-policy and size-admission behavior.
- Missing, unsupported, rejected, or too-large media must render existing fallback states without creating skeleton transcript rows.
- Active media preview or context menu pins must remain valid until their owning affordance closes or is rejected.

Verification:

- Unit coverage for coordinator budgeting, coalescing, cancellation, stale result rejection, and cache hits.
- Integration coverage showing prepaint no longer walks offscreen Markdown/media runs.
- Existing media, Markdown image, generated-image, and visible-media diagnostics tests.
- Live frame metrics showing media preload cost is separated from row prepaint and remains bounded during scroll.

# Phase 3: Transcript Row Presentation Models And Measurement Revisions (finished)

Hard task:

Introduce row presentation models and explicit measurement revision keys so visible-row render/layout consumes precomputed row facts instead of rediscovering turn structure and invalidating measurements broadly.

Implementation notes:

- Derive row models from resident turns, live stream projections, Markdown keys, media descriptors, code-panel ids, copy-source spans, selection metadata, stable row identity, source turn identity, item counts, text counts, and revision metadata.
- Update row models when resident turn data changes, stream projection changes, Markdown parse results change, media layout-affecting state changes, or edit/live-scroll state changes.
- Key measurement cache entries by row identity, row presentation revision, transcript width, theme/font revision, and layout-affecting display state.
- Keep row rendering responsible for GPUI element construction only for visible rows and bounded overscan.

Edge cases:

- Same backend turn with multiple user fragments must preserve fragment order and row identity.
- Active-turn steering fragments must append at the accepted narrative position without starting a new reading cycle.
- Thread activation must publish initial row models and initial viewport state in one transaction.
- Width, theme, font, media promotion, code-panel resize, and Markdown parse updates must invalidate only affected measurements.
- Very large single turns must expose block-level model boundaries or an explicit follow-up if row-level caching is insufficient.

Verification:

- Unit coverage for row model derivation from historical and live turns.
- Tests proving row identity and source turn identity survive prepends, appends, releases, and restores.
- Measurement invalidation tests for width/theme/font/media/code-panel changes.
- Existing selection, copying, quote, context-menu, edit, live-scroll, and thread-activation tests.
- Live debug-build scroll sample on a long thread showing lower row-build and row-prepaint cost.

# Phase 4: Typed Row-Owned Code Panel State (finished)

Hard task:

Replace string-scanned transcript code-panel ownership with typed row-owned panel identity and row-scoped pruning.

Implementation notes:

- Introduce a typed transcript code-panel identity containing row identity plus local panel identity.
- Store nested scroll ownership, soft-wrap state, resized heights, scrollbar visibility, syntax/projection ownership, and rendered-panel tracking against typed identities or row-scoped maps.
- Prune code-panel state by bounded visible/protected row identity sets rather than scanning all panel ids against all visible row strings.
- Preserve nested code-panel wheel ownership and selected-panel behavior.

Edge cases:

- Panel identity must remain stable when older rows are prepended and presentation indexes shift.
- Releasing a resident row must release unpinned row-owned panel state without disturbing visible rows.
- Open context menus, active selection, and selected nested code panel state must close or transfer only according to existing UI contracts.
- Duplicate code blocks within one row must get distinct local panel identities.
- Theme and syntax-highlight cache invalidation must still target the correct panel owner.

Verification:

- Unit coverage for typed identity parsing/construction if any conversion boundary remains.
- Tests for code-panel soft-wrap, resize, nested wheel ownership, release pruning, and transcript reset.
- Source guard proving code-panel pruning no longer scans panel strings against every visible row identity.
- Existing Markdown/code-panel selection and copy tests.

# Phase 5: Scrollbar And Transcript Invalidation Coalescing (finished)

Hard task:

Make scrollbar activity and transcript scroll notifications state-change aware so hover, wheel, fade, and thumb activity do not force broad transcript recomputation when content viewport state has not meaningfully changed.

Implementation notes:

- Separate scrollbar chrome invalidation from transcript content invalidation.
- Notify transcript content only when visible range, scroll position class, resident boundary state, anchor state, or relevant scrollbar visibility state changes.
- Coalesce repeated pointer-move and wheel activity within the same effective scrollbar state.
- Keep direct thumb dragging and lane-click behavior routed through the owning scroll surface.

Edge cases:

- Pointer movement over transcript should still reveal the scrollbar when overflow exists.
- Nested code-panel scrollbars must still reveal independently without stealing transcript wheel ownership until selected.
- Manual scroll intent must still detach live autoscroll before residency range work.
- Boundary clamp attempts must still trigger policy-allowed resident loading.
- Scrollbar fade animation must not starve content rerender when content actually changes.

Verification:

- Unit or integration coverage for notification coalescing and transcript content invalidation gates.
- Existing live-scroll, virtual-tail, nested code-panel, and resident-boundary tests.
- Live scroll metrics showing fewer redundant transcript frames during pointer movement and wheel scrolling.

# Phase 6: Long-Thread Performance Verification And Completion Review (wip)

Hard task:

Verify the completed architecture on representative long-thread scenarios, close regressions found by diagnostics, and obtain completion review.

Implementation notes:

- Capture frame metrics for long text rows, Markdown-heavy rows, code-panel-heavy rows, generated-image rows, and resident-boundary scrolling.
- Compare frame metrics before and after the architecture changes using content-free diagnostics.
- Confirm retained state stays bounded while rows are loaded and unloaded.
- Run the required reviewer subagent after all phases are implemented because authoritative docs and code are touched.

Current live-test issue:

- June 11, 2026 reproduction confirmed that a debug diagnostic child against the isolated copied home `C:\Users\user\AppData\Local\Temp\beryl-phase6-home-20260611-142119` can make Windows barely responsive while the `City Image Generation` thread is selected. The child showed ten visible source-backed generated images and transcript frame metrics with repeated large media-preload spikes, including hundreds of milliseconds and an approximately 1.4 second single-frame sample. Do not repeat the generated-image live diagnostic path without stricter guardrails, and treat Phase 6 generated-image verification as unresolved until the preload/render loop is investigated safely.
- Phase 7 is the planned remediation for this blocker. Phase 6 cannot be cleared until Phase 7 implementation and verification show that image-heavy selected-thread activation and scroll-boundary admission do not rely on render-time completed-media loading.

Edge cases:

- Debug build scrolling must be acceptable for long resident windows and for a single large visible turn.
- Resident byte and turn budgets must still explain smaller-than-target margins through diagnostics.
- Frame metrics must not log transcript text, paths, secrets, or unbounded identifiers.
- Explicit diagnostic tools must remain bounded and must not load extra history or media solely to answer diagnostics.

Verification:

- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- `cargo nextest run -p beryl-app`
- Source guards for no old skeleton/detail-loading architecture assumptions.
- Live diagnostic samples from the operator's long-thread scroll path, including frame metrics, retained state summary, renderer diagnostics, visible media, and memory diagnostics.
- Completion review subagent findings addressed or planned before clearing this file.

# Phase 7: Presentable Media Admission Gates (wip)

Hard task:

Move completed transcript-media readiness out of visible render/prepaint/deferred frame work and into transcript residency/admission, so a selected transcript row becomes visible or scrollable only after full-detail turn data, row presentation state, and required media resources are ready or terminal fallback states are known.

Implementation notes:

- Completed implementation slice: exposed exact source-backed image request status from the local GPUI fork. Beryl render now consumes cache-displayed media without scheduling loads, while the preload path owns media load scheduling and source-backed readiness requests.
- Next implementation slice: introduce explicit presentability state for planned row media and move selected-thread activation plus scroll-boundary history publication behind that state.
- Introduce an explicit presentability state for transcript rows or row blocks. A row is presentable only after full-detail turn data, row presentation model, Markdown/media plans, and completed-media readiness are settled for its planned viewport presentation.
- Split completed-media admission from speculative preload. Speculative preload may prepare future rows, but visible historical or completed rows must consume settled media descriptors and must not start or complete media readiness as part of ordinary frame construction.
- For selected-thread activation and workspace restore, keep the prior coherent transcript or startup gate visible until the initial selected-thread viewport is presentable, including source-backed generated images at the planned layout size or terminal fallback states.
- For scrolling into offscreen history, clamp at the resident or presentable boundary, coalesce the requested direction or target, perform bounded admission work, and extend the scrollable range atomically only after the target rows are presentable.
- Track source-backed media readiness by stable row identity, media source revision, path identity, requested render size, window scale, and row presentation revision. Reject stale completions after thread switches, row releases, source changes, resize-driven size changes, or media promotion changes.
- Keep GPUI ownership clear: Beryl owns admission/readiness leases, terminal fallback state, cancellation, diagnostics, and byte/count accounting; `gpui` owns decoded/uploaded source-backed rendering resources. Saved-path-backed generated images remain source-backed and Beryl must not retain full file bytes as durable GUI-owned state.
- Treat file-unavailable, unsupported, path-disallowed, too-large, decode-failed, or admission-failed results as terminal presentable fallback states. Treat pending filesystem reads, decode, upload, or source-backed preload requests as non-presentable for historical or completed rows.
- Preserve the live-turn exception: a backend item whose image generation is genuinely pending may render a stable pending placeholder. Once the backend item is completed or historical, Beryl must not publish it as a placeholder solely because local media readiness is still pending.
- Bound background work by row count, media item count, in-flight request count, file/decode/upload bytes, elapsed time, and cancellation generation. Repeated unchanged frames must not enqueue or request the same source-backed image readiness again.

Edge cases:

- A single visible turn containing many saved-path generated images, including the `City Image Generation` shape with ten source-backed images.
- Selected-thread activation from an existing coherent thread into an image-heavy historical tail.
- Workspace startup restore directly into an image-heavy selected thread.
- Pointer-wheel, scrollbar, keyboard, and programmatic scroll attempts beyond a non-presentable resident boundary.
- Source paths that disappear, become unreadable, exceed limits, or fail decode during admission.
- Thread switches, workspace switches, window scale changes, transcript width changes, media promotion changes, and row release while media readiness is in flight.
- Memory or resource budgets that prevent admitting the full target margin; Beryl should keep the current coherent viewport, admit a smaller safe range when possible, and record a bounded diagnostic reason.
- Diagnostic reads and diagnostic child observation tools must not load history, read media files, decode images, or create renderer resources solely to answer diagnostics.

Verification:

- Unit or integration coverage for row presentability state transitions, terminal media fallback admission, stale completion rejection, and cancellation on thread or workspace switch.
- Selected-thread activation tests proving the old coherent projection remains visible until the new initial viewport is presentable.
- Scroll-boundary tests proving attempts to enter media-pending rows clamp until admission completes, then extend the scrollable range once.
- Source guards or focused tests proving ordinary transcript render, prepaint, frame-metric, status-line, and scrollbar paths do not initiate completed-media readiness for visible rows.
- City-like fixture coverage with a historical turn containing many source-backed generated images, verifying no visible row publication before all images are ready or terminal fallbacks are known.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- `cargo nextest run -p beryl-app`
- Do not repeat the live debug diagnostic child generated-image repro until the implementation has passed automated verification and the live run has explicit guardrails: a release or otherwise low-overhead build, isolated child home, bounded test thread, process diagnostics sampled before activation, immediate stop criteria, and operator approval for that specific live run.
