# Scope

Implement the transcript scroll/render responsiveness architecture described by `doc/features/transcript/design.md`, `doc/ui.md`, `doc/features/diagnostics/design.md`, and `crates/beryl-app/doc/design.md`.

The implementation must keep transcript residency, render snapshots, media admission, speculative media preload, row presentation, code-panel state, scrollbar invalidation, and diagnostics on clean ownership boundaries. Ordinary transcript render, prepaint, scroll, scrollbar, status-line, and frame-metric paths must not scan resident or indexed history, parse offscreen Markdown, schedule resident history loads, initiate completed-media readiness for visible rows, or perform broad retained-state diagnostics.

The starting point is the current resident-window transcript architecture. Preserve its user-visible behavior: existing-thread activation publishes coherent presentable resident content atomically, scrolling clamps at nonresident or media-pending boundaries, loaded turns extend on demand only after their rows are presentable, released turns do not render skeleton or loading rows, semantic scroll anchors are preserved, and transcript selection/quote/context-menu behavior remains correct for resident content.

Readiness:

- Design authority has been updated before implementation.
- No migration adapters or compatibility branches for old skeleton/detail loading are allowed.
- If a planned phase turns out technically invalid, stop and report the contradiction instead of inventing a workaround.

Resumable milestone:

- Continue Phase 6 live long-thread verification and completion closeout. Phase 12 fixed the completion-review blockers, but the generated-image live diagnostic path still requires explicit guardrails and operator approval before it is repeated.
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

# Phase 6: Long-Thread Performance Verification And Completion Review (finished)

Hard task:

Verify the completed architecture on representative long-thread scenarios, close regressions found by diagnostics, and obtain completion review.

Implementation notes:

- Capture frame metrics for long text rows, Markdown-heavy rows, code-panel-heavy rows, generated-image rows, and resident-boundary scrolling.
- Compare frame metrics before and after the architecture changes using content-free diagnostics.
- Confirm retained state stays bounded while rows are loaded and unloaded.
- Run the required reviewer subagent after all phases are implemented because authoritative docs and code are touched.

Current live-test issue:

- June 11, 2026 reproduction confirmed that a debug diagnostic child against the isolated copied home `C:\Users\user\AppData\Local\Temp\beryl-phase6-home-20260611-142119` can make Windows barely responsive while the `City Image Generation` thread is selected. The child showed ten visible source-backed generated images and transcript frame metrics with repeated large media-preload spikes, including hundreds of milliseconds and an approximately 1.4 second single-frame sample. Do not repeat the generated-image live diagnostic path without stricter guardrails, and treat Phase 6 generated-image verification as unresolved until the preload/render loop is investigated safely.
- Phases 7 through 11 are the planned remediation for this blocker. Phase 6 cannot be cleared until those phases and verification show that image-heavy selected-thread activation and scroll-boundary admission do not rely on render-time completed-media loading.
- June 12, 2026 completion review found two blockers after Phase 11: staged admission can deadlock when the current published transcript is empty, and user-input Markdown image candidates can publish without staged media admission. Phase 12 owns these fixes.
- June 12, 2026 Phase 12 fixed the completion-review blockers and focused re-review found no remaining blocker. Phase 6 still cannot be cleared until the live generated-image verification is repeated with explicit guardrails and operator approval.

Pending guarded live-run request:

- June 12, 2026: release executables exist at `target\release\beryl.exe` and `C:\Users\user\apps\bin\beryl-standalone.exe`; the proposed generated-image verification should use `target\release\beryl.exe`.
- The run must use a copied isolated Beryl home, confirm the exact `City Image Generation` thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2` before activation, sample baseline process/memory/renderer/frame diagnostics before activation, activate that thread once, capture one bounded image-visible sample, then switch away or stop the child immediately.
- Abort criteria: diagnostic child tool timeout, shell-response timeout, sustained media-preload spike, unexpected visible-media count, unexpected thread identity, missing fixture, renderer attribution failure, or visible operator-machine responsiveness degradation.
- Do not start this generated-image diagnostic run until the operator explicitly approves this specific guarded run.
- June 12, 2026 guarded release attempt: copied `C:\Users\user\.beryl` to `C:\Users\user\AppData\Local\Temp\beryl-phase6-guarded-home-20260612-055622` and started `target\release\beryl.exe` as diagnostic child PID `336`. The first readiness wait for `workspace_idle` timed out before fixture activation, so the abort criterion fired and the child was stopped. No generated-image thread was activated and no image-visible sample was captured. A later attempt should use readiness predicates that match the required pre-activation state without imposing a short poll limit, then keep the same activation and immediate-stop guardrails.
- June 12, 2026 guarded City release attempt: copied `C:\Users\user\.beryl` to `C:\Users\user\AppData\Local\Temp\beryl-phase6-city-home-20260612-055824` and started `target\release\beryl.exe` as diagnostic child PID `31920`. Baseline renderer attribution was ready with zero source-backed image resources, zero visible media, private bytes `76,738,560`, and working set `67,469,312`. The copied inventory contained the exact `City Image Generation` fixture thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`; activation selected that thread, settled with no background work, and reported ten visible loaded native generated-image items, all source-backed files at natural size `1536x1024` and displayed size `419.0625x279.375`. Image-visible renderer diagnostics showed source-backed request/live counts `10/10`, final-scene resource count `4`, preload live count `6`, pending decode/upload counts `0/0`, failed source count `0`, image resource count `10`, live GPU bytes `10,383,360`, upload count `10`, and decoded CPU estimate `0`. Image-visible memory was private bytes `102,141,952` and working set `93,515,776`. Retained state stayed bounded with `loadedTranscriptTurns = 2`, `presentationRows = 2`, `mediaCacheEntries = 10`, `mediaCacheLoadedSourceBackedFileEntries = 10`, `mediaCacheLoadedImageBytes = 0`, and `mediaCacheDecodedImageBytesEstimate = 0`. Image-visible frame metrics in the returned bounded window reported frame samples around `503` to `1,977` microseconds, media run render up to `616` microseconds, and media preload mostly around `108` to `474` microseconds, with no repeat of the previous hundreds-of-milliseconds or 1.4-second preload spike. The child was stopped immediately after the bounded sample and lifecycle status returned `not_running`.

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

Completion note:

- June 12, 2026: Phase 6 closeout passed after the guarded release diagnostic child run on the `City Image Generation` fixture showed ten loaded source-backed generated images without repeating the previous debug-child media-preload spike or responsiveness failure. Automated verification also passed with `cargo fmt -p beryl-app`, `cargo check -p beryl-app --tests`, and `cargo nextest run -p beryl-app` with `1687` tests passed. Completion-review blockers found after Phase 11 were fixed in Phase 12 and the focused reviewer re-check found no remaining blocker.

# Phase 7: Staged Selected-Thread Publication (finished)

Hard task:

Refactor selected-thread activation, workspace startup restore, and backend-reopen refresh so loaded selected-thread history is staged as a pending activation record instead of publishing transcript rows immediately.

Implementation notes:

- Move direct selected-history publication out of `ConversationSurfaceState::seeded`, `ConversationSurfaceState::refresh_after_backend_reopen`, and `ShellView::finish_thread_activation_worker`.
- Add a staged selected-thread admission record that owns the loaded `ThreadInfo`, `TranscriptHistoryWindow`, image resolver, session metadata, activation source, and intended initial viewport policy.
- Preserve the previous coherent selected transcript while the staged activation is pending.
- Keep activation progress in chrome/status/notices rather than replacing the transcript with a full-region loading state.
- Factor the existing `load_thread_history_window` body into a publish helper that is called only after later admission phases accept the staged record.

Edge cases:

- Workspace startup restore directly into an existing selected thread.
- Backend reopen with a preserved coherent surface and newly loaded selected-thread history.
- Explicit thread selector, graph link, branch-and-switch, decision handoff, and exact thread-link activation.
- Activation failure, requires-rebind, backend unavailable, thread switch supersession, and workspace switch while a staged activation exists.
- Active edit mode, context menus, quote popup, media popup, and composer image-label sync must cancel or remain scoped to the still-visible coherent thread until staged publication succeeds.

Verification:

- Source guards proving selected-thread activation paths stage records before calling the publish helper.
- Tests proving pending selected-thread activation preserves the old coherent projection until explicit staged publication.
- Tests proving stale staged activations are rejected after thread or workspace switch.
- Existing activation, thread navigation, branch switch, decision handoff, startup restore, and backend reopen tests.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- Focused `cargo nextest run -p beryl-app` tests for selected-thread activation paths.

Completion note:

- June 12, 2026: selected-thread activation now stages startup restore, backend-reopen refresh, and activation-worker results through a surface-owned staged record before explicit publication. Phase 7 keeps an immediate staged-publication acceptance point so selected-thread activation remains functional until later presentability admission phases replace it.

# Phase 8: Staged Residency-Page Publication (finished)

Hard task:

Refactor scroll-boundary history loading so completed residency-page worker results are staged as pending page admissions instead of prepending or restoring transcript rows immediately.

Implementation notes:

- Split the current residency-page completion path into worker result staging and later publish.
- Stage both older-page loads and released-page restores with request identity, loaded page data, image resolver, source range, visible-range intent, and cancellation generation.
- Keep the scrollable transcript range clamped at the current resident/presentable boundary while staged page admission is pending.
- Preserve the existing page release path and INFO load/unload logging, but emit load logs only when staged page publication actually admits turns.
- Keep one in-flight residency-page admission per selected thread unless a later policy explicitly allows more.

Edge cases:

- Multiple wheel, scrollbar, keyboard, and programmatic scroll attempts beyond the same boundary while one page is staged.
- Superseding an older-page request with a released-page restore request, and the reverse.
- Worker failure, disconnected worker, backend unavailable, selected-thread switch, workspace switch, and released page metadata pruning while a page is staged.
- Releasing cold pages immediately after a staged page publishes must not release the newly visible viewport page.
- Semantic scroll anchors and detached manual positions must remain valid while the page is staged and after it publishes.

Verification:

- Source guards proving `finish_thread_history_page_worker` stages before publication.
- Tests proving scroll attempts clamp while a staged page is pending and extend once after publication.
- Tests proving stale page admissions are rejected after thread, workspace, or request-generation changes.
- Existing transcript history release/restore, scroll-boundary, live-scroll, and presentation mutation tests.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- Focused `cargo nextest run -p beryl-app` transcript history and scroll tests.

Completion note:

- June 12, 2026: scroll-boundary residency-page worker completions now pass through a shell-held request ticket and surface-owned staged page admission before explicit publication. The staged record carries request identity, loaded page data, image resolver, source/presentation range intent, and cancellation generation; load logging and row mutation happen only from the publish helper. Phase 8 keeps immediate staged publication so current scrolling behavior remains functional until Phase 11 gates publication on presentability.

# Phase 9: Row Presentability State Model (finished)

Hard task:

Introduce an explicit row or row-block presentability state that classifies staged transcript rows by full-detail availability, row presentation readiness, Markdown/media planning readiness, and completed-media readiness.

Implementation notes:

- Model presentability outside ordinary render, prepaint, status-line, frame-metric, and scrollbar paths.
- Key media readiness by row identity, media source revision, path identity, requested render size, window scale, and row presentation revision.
- Treat unavailable, unsupported, path-disallowed, too-large, decode-failed, and admission-failed media as terminal presentable fallback states.
- Treat pending filesystem reads, decode, upload, or source-backed preload requests as non-presentable for historical or completed rows.
- Preserve the live-turn exception: genuinely pending backend image generation may render a stable pending placeholder, but completed or historical generated-image rows may not publish solely as local pending placeholders.

Edge cases:

- Rows with no media, Markdown-only rows, inline byte-backed generated images, saved-path source-backed generated images, and local Markdown images.
- A single turn with many generated images, including the `City Image Generation` shape.
- Source changes, row release, media promotion changes, transcript width changes, theme/font changes, and window scale changes.
- Missing or stale Markdown/media plans must not create skeleton transcript rows or fake loading rows.

Verification:

- Unit coverage for presentability state transitions and terminal fallback states.
- Tests proving live pending generated images are the only allowed pending-placeholder exception.
- Tests proving presentability keys reject stale media completions after row/source/layout revision changes.
- Source guards proving presentability planning is not run from ordinary render/prepaint/status/frame paths.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- Focused `cargo nextest run -p beryl-app` presentability and media-source tests.

Completion note:

- June 12, 2026: introduced the shell-owned transcript presentability model and per-media readiness keys. Staged selected-thread activations and residency-page admissions now retain a presentability window derived outside render, prepaint, status-line, frame-metric, and scrollbar paths. The model records full-detail, row-presentation, Markdown/media-plan, and completed-media readiness; terminal fallbacks are presentable; the live pending generated-image placeholder exception is explicit. Phase 9 keeps immediate publication so Phase 10 can add the admission driver.

# Phase 10: Window-Backed Media Admission Driver (finished)

Hard task:

Implement a bounded window/layout-backed media admission driver that advances staged selected-thread rows and staged residency pages to presentable or terminal fallback state before publication.

Implementation notes:

- Admission consumes staged rows, row models, Markdown/media plans, transcript width, viewport height, theme metrics, window scale, and source-backed image request status.
- Admission may schedule bounded file/decode/upload work, but repeated unchanged frames must not enqueue the same readiness request again.
- Bound work by row count, media item count, in-flight request count, requested upload bytes, elapsed time, cancellation generation, and memory/resource budget.
- Beryl owns admission leases, stale-result rejection, terminal fallback state, diagnostics, and accounting; GPUI owns decoded/uploaded source-backed rendering resources.
- Diagnostic reads and diagnostic child tools must not load history, read media files, decode images, or create renderer resources solely to answer diagnostics.

Edge cases:

- Admission budget exhaustion for image-heavy rows.
- Source files disappearing, becoming unreadable, exceeding limits, failing decode, or being rejected by path policy while admission is in flight.
- Thread switches, workspace switches, resize, scale change, media promotion, row release, and cache eviction while admission is in flight.
- Admission should publish a smaller safe range only when that range is coherent and explicitly recorded by diagnostics.

Verification:

- Unit or integration coverage for admission budgeting, cancellation, stale completion rejection, and fallback admission.
- Tests proving unchanged frames do not reschedule identical source-backed readiness work.
- City-like fixture coverage proving an image-heavy historical row reaches presentable or terminal fallback state before publication.
- Diagnostic tests proving admission state is content-free and bounded.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- Focused `cargo nextest run -p beryl-app` media admission tests.

Completion note:

- June 12, 2026: staged selected-thread activations and staged residency-page admissions now own a completed-media admission window, and the transcript panel drains a bounded window/layout-backed driver before speculative preload. The driver advances generated-image media through cache lookup, bounded load scheduling, source-backed image request status, upload preloading leases, terminal fallback accounting, and content-free summaries without using ordinary render paths or diagnostics to initiate readiness work. Focused admission-summary and source-guard coverage now tracks no-media rows, image-heavy rows, saved-path generated images, staging wiring, source-backed lease status, and admission-before-preload ordering.

# Phase 11: Presentable Publication Gates And Render-Path Cleanup (finished)

Hard task:

Gate selected-thread activation and scroll-boundary history extension on staged presentability, and remove completed-media readiness initiation from ordinary transcript render/prepaint/deferred frame work.

Implementation notes:

- Publish staged selected-thread activation only after the initial viewport and required resident runway are presentable.
- Publish staged residency pages only after the target boundary rows are presentable, then extend the scrollable range atomically.
- Clamp pointer-wheel, scrollbar, keyboard, and programmatic scroll attempts at resident or non-presentable boundaries while admission is pending.
- Keep speculative preload for future rows, but visible historical or completed rows must consume settled descriptors and must not initiate completed-media readiness during ordinary frame construction.
- Preserve semantic anchors, detached manual positions, selection/quote/context-menu behavior, and code-panel state for resident content.
- Keep source-backed generated images file-backed when possible and avoid retaining full saved-path bytes as durable GUI-owned state.

Edge cases:

- Selected-thread activation from a coherent thread into an image-heavy historical tail.
- Workspace startup restore directly into an image-heavy selected thread.
- Scroll attempts into a staged page while the current visible rows are also being released or resized.
- Memory/resource budgets preventing the full target margin from becoming presentable.
- Concurrent active turn, edit commit, branch activation, context compaction, or thread steering work while admission is pending.

Verification:

- Selected-thread activation tests proving old coherent projection remains visible until the new initial viewport is presentable.
- Scroll-boundary tests proving attempts into media-pending rows clamp until admission completes, then extend once.
- Source guards proving render, prepaint, frame-metric, status-line, and scrollbar paths do not initiate completed-media readiness for visible rows.
- Existing transcript selection, copying, quote popup, context menu, edit, live-scroll, code-panel, and media tests.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- `cargo nextest run -p beryl-app`
- Do not repeat the live debug diagnostic child generated-image repro until automated verification passes and the live run has explicit guardrails: release or otherwise low-overhead build, isolated child home, bounded test thread, process diagnostics sampled before activation, immediate stop criteria, and operator approval for that specific live run.

Completion note:

- June 12, 2026: selected-thread activation and staged residency-page publication now gate on structural row readiness plus settled staged media admission before taking and publishing staged records. The transcript deferred admission pass is the acceptance point that records media-admission summaries and publishes only after admission settles. Markdown image candidates and generated images are included in the staged media-admission count, visible rows are filtered out of speculative media preload, and source guards cover the render/prepaint/deferred cleanup boundary. Verification passed with `cargo fmt -p beryl-app`, `cargo check -p beryl-app --tests`, focused nextest coverage, and full `cargo nextest run -p beryl-app`.

# Phase 12: Completion Review Remediation (finished)

Hard task:

Fix the completion-review blockers that prevent Phase 6 from clearing.

Implementation notes:

- Schedule the staged media-admission drain even when the currently published transcript projection is empty, so startup restore and empty-surface activation can advance pending selected-thread admission.
- Keep the drain outside ordinary visible-row render work and keep completed-media readiness initiation inside the staged admission driver.
- Add user-input Markdown image candidates to row media descriptors using the same stable Markdown key and source revision used for the user prompt Markdown source.
- Preserve current behavior for empty user prompts and prompt image-marker attachments.

Edge cases:

- Startup restore directly into an image-heavy historical thread when no prior selected transcript is published.
- Empty current surface with a staged selected-thread activation whose rows require media admission before publication.
- User prompt text containing Markdown image syntax without attached prompt image markers.
- User prompt text with attached image markers must not double count Markdown-media admission for the prompt image-marker path.

Verification:

- Source guard proving staged media admission is scheduled from the empty-state transcript path when a staged request exists.
- Unit coverage proving user-input Markdown image candidates contribute to staged media admission counts.
- Existing source guards for visible-row preload exclusion and admission-before-preload ordering.
- `cargo fmt -p beryl-app`
- `cargo check -p beryl-app --tests`
- `cargo nextest run -p beryl-app`

Completion note:

- June 12, 2026: empty-state transcript rendering now schedules the staged media-admission drain in a deferred prepaint pass, so startup restore and empty-surface selected-thread activation can advance staged publication even before any rows are published. User-input Markdown image syntax now contributes a `MarkdownImageCandidate` media descriptor when the prompt has no prompt image-marker attachments, preserving the attachment path without double counting. Added source-guard coverage for the empty-state staged-admission hook and unit coverage for user-input Markdown image admission. Verification passed with `cargo fmt -p beryl-app`, focused `cargo nextest run -p beryl-app media_admission_marks_user_markdown_image_candidates_pending completed_media_admission_is_window_backed_and_staged`, `cargo check -p beryl-app --tests`, full `cargo nextest run -p beryl-app`, and focused reviewer re-check with no blocking findings.
