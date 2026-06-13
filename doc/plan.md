# Scope

Replace the current mixed outer-list plus streamed-row navigation architecture with a single transcript viewport frame owner for continuous transcript scrolling.

The work is limited to transcript viewport state, frame construction, continuous wheel/touchpad navigation, streamed chunk and ordinary row rendering, local segment measurement, residency fill facts, diagnostics, and automated tests in `crates/beryl-app`. It must preserve the existing design constraints from `doc/features/transcript/design.md`: whole-turn historical residency, active live-turn source pinning, no transcript visual scrollbar, no unrendered huge-turn spacer geometry, no dedicated chunk navigation commands, no render-anyway control, and viewport-local selection/copy for streamed chunks.

Readiness: live testing showed two architecture failures. First, continuous scrolling from the latest turn into the previous offscreen turn can jump far into the previous turn, because a streamed boundary crossing is translated into row or chunk placement instead of preserving the wheel delta and visual anchor. Second, continued scrolling can oscillate vertically, because measurement updates, list row invalidation, and semantic anchor refill are still coupled through competing scroll owners.

Target architecture: the transcript viewport owns one rendered-frame model spanning ordinary rows and streamed chunks. Continuous wheel and touchpad input moves through that rendered frame by pixel delta, lazily prepending or appending adjacent resident content when the frame edge is reached. Explicit placement remains only for selected-thread activation, tail/bottom commands, `PageUp`, `PageDown`, and turn-to-turn commands.

Resumable milestone: new frame-owner architecture is planned; implementation has not started.

# Phase 1: Establish Transcript Frame Ownership (pending)

Task:

- Introduce a transcript viewport frame model that represents visible and overscan content segments across ordinary rows and streamed chunks.
- Define stable segment identities for ordinary rows, streamed chunks, render-budget fallback chunks, and terminal resident-budget fallback rows.
- Make the frame model the only continuous-scroll authority for transcript wheel and touchpad input.
- Identify and remove design reliance on `scroll_transcript_streamed_row_to_placement` and any equivalent row placement helper for continuous scroll.

Edge cases:

- Latest ordinary turn with the previous turn just above the viewport.
- Latest streamed huge turn with the previous ordinary turn just above the viewport.
- Latest ordinary turn with the previous streamed huge turn just above the viewport.
- Adjacent streamed huge turns.
- Empty transcript, single-row transcript, and resident-boundary clamp.

Verification:

- Add pure frame-model tests proving adjacent segment lookup works across ordinary row and streamed chunk boundaries.
- Add source guards proving continuous wheel/touchpad paths do not call row-to-top or row-to-bottom placement helpers.
- Add source guards proving transcript visual scrollbar remains absent.

Resumable milestone: transcript frame state can describe continuous visible content without delegating scroll ownership to the outer virtual list.

# Phase 2: Implement Delta-Preserving Continuous Scroll Reducer (pending)

Task:

- Replace streamed boundary wheel/touchpad reduction with a frame reducer that consumes pixel deltas locally, expands the frame in the scroll direction when needed, and preserves the visual anchor plus any remaining delta.
- Treat resident history boundaries as clamps that request residency work without entering unloaded or fake rows.
- Keep live-tail detachment on manual scroll intent before boundary and residency decisions.

Edge cases:

- Small delta crossing a turn boundary by only a few pixels.
- Large wheel delta crossing multiple chunks or rows.
- Touchpad deltas that accumulate over multiple events.
- Boundary clamp at oldest or newest resident content.
- Manual scroll while live autoscroll is following tail.

Verification:

- Add pure reducer tests for no-jump boundary crossing, multi-segment delta consumption, residual delta handling, clamps, and live-tail detachment.
- Add production bridge tests proving wheel/touchpad input goes through the frame reducer before any fallback.

Resumable milestone: continuous scroll advances through rows and chunks by delta rather than by semantic placement.

# Phase 3: Render From Transcript Frames (pending)

Task:

- Replace the transcript row-list render path for the transcript region with a frame renderer that renders only frame segments plus bounded overscan.
- Preserve existing row visuals so adjacent chunks from the same turn still read as one turn to the user.
- Keep ordinary rows on whole-row segment rendering and huge turns on chunk segment rendering.
- Preserve nested code-panel wheel ownership above transcript scrolling.

Edge cases:

- Same-turn chunks split across the top and bottom of the viewport.
- Turn card chrome and context-menu targeting across multiple visible chunks of one turn.
- Render-budget fallback chunk inside a streamed turn.
- Nested code panel selected for wheel ownership while its outer streamed chunk remains visible.

Verification:

- Add render/source tests proving the transcript region renders from frame segments, not an outer variable-height list scroll extent.
- Add context-menu and code-panel tests proving visible segment rendering still targets the owning backend turn and nested code panel.

Resumable milestone: the transcript surface renders bounded frame segments directly.

# Phase 4: Transactional Segment Measurement (pending)

Task:

- Replace ad hoc row/chunk measurement invalidation with a staged segment-measurement queue committed once per frame.
- Coalesce measurements by segment key and display/layout revision before mutating viewport state.
- Preserve the current visual anchor when measured heights change, and forbid repeated corrections that oscillate visible text.

Edge cases:

- Chunk height changes after first measurement.
- Media readiness changes a visible or overscan segment height.
- Width, theme, font, or code-panel state invalidates segment measurements.
- Same measurement reported repeatedly.
- Multiple segment measurements arrive in one frame.

Verification:

- Add pure measurement-commit tests proving coalescing, no-op same-height updates, anchor preservation, and no oscillating correction loop.
- Add source guards proving segment measurement callbacks do not synchronously mutate transcript scroll state during prepaint.

Resumable milestone: measured segment heights refine future frames without causing vertical oscillation.

# Phase 5: Reconnect Residency To Frame Facts (pending)

Task:

- Feed residency planning from frame facts: visible source range, leading and trailing rendered overscan, resident boundary clamps, and streamed segment fill facts.
- Ensure renderer, prepaint, deferred callbacks, media preload, diagnostics, and status-line code report facts only; the residency controller remains the only loader/releaser.

Edge cases:

- Frame reaches resident start while scrolling upward and must clamp while requesting older resident turns.
- Frame reaches resident end while detached from live tail.
- One huge streamed turn satisfies the configured viewport margin.
- Several small ordinary turns are needed to satisfy the configured viewport margin.
- Resident-window release preserves enough semantic identity and measured facts for stable frame reconstruction.

Verification:

- Add residency planner tests driven by frame facts for ordinary rows, streamed chunks, clamps, and budget-limited margins.
- Add source guards proving non-controller paths do not schedule history loads or releases.

Resumable milestone: residency expansion and release follow frame-owned viewport facts.

# Phase 6: Rebuild Explicit Navigation On Frame Anchors (pending)

Task:

- Reimplement selected-thread activation, bottom/tail command, `PageUp`, `PageDown`, and turn-to-turn commands as explicit frame-anchor placements.
- Keep these commands distinct from continuous wheel/touchpad delta handling.
- Preserve live prompt, commentary, final-start, and passive final-runway semantics through frame anchors rather than outer-list offsets.

Edge cases:

- PageUp/PageDown with ordinary rows only.
- PageUp/PageDown with a streamed huge turn.
- Ctrl+Up/Ctrl+Down targeting ordinary and huge turns.
- Existing-thread activation at tail with passive final runway.
- Active live-turn prompt and final-start anchoring with detached manual scroll.

Verification:

- Add command tests proving explicit navigation may place anchors while wheel/touchpad cannot.
- Rerun live-scroll, selected-thread activation, transcript scroll, and viewport suites.

Resumable milestone: all non-continuous navigation commands target frame anchors.

# Phase 7: Diagnostics And Regression Review (pending)

Task:

- Update content-free diagnostics and frame metrics to report frame anchor, visible segment range, overscan segment count, segment measurement commit counts, clamp reasons, and residency requests.
- Run focused and broad regression suites.
- Request completion-review subagent after implementation phases finish.

Regression targets:

- Scrolling upward from the latest turn into a previous offscreen turn has no visual jump.
- Scrolling through many streamed chunks in either direction has no vertical oscillation.
- Adjacent ordinary and streamed turns fill the viewport plus overscan.
- Render-budget fallback chunks do not become scroll spacers.
- Selection and quote behavior remain viewport-local for streamed chunks.
- Nested code-panel scrolling remains isolated.
- Live-tail detach, passive final runway, and selected-thread activation still behave correctly.

Verification:

- Run focused `cargo nextest run -p beryl-app` suites for transcript viewport, transcript scroll, transcript presentation, transcript history, live scroll, selection/copy/quote, nested code panels, diagnostics, and source guards.
- Run broader `cargo nextest run -p beryl-app` if focused suites pass and runtime is acceptable.
- Use live Beryl or diagnostic-child testing on the reported thread shape before finalizing.
- Address reviewer findings through an updated plan before clearing `doc/plan.md`.

Resumable milestone: frame-owned transcript scrolling passes automated and live regression review.
