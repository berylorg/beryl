# Scope

Implement measured chunked rendering for very large transcript turns while preserving whole-turn residency semantics from `doc/features/transcript/design.md`.

The work is limited to transcript presentation, rendering, measurement, residency integration points, diagnostics, and automated tests in `crates/beryl-app`. It must not change the CAS history contract, load partial backend turns, introduce synthetic transcript rows, or make source-size heuristics authoritative for visible scroll geometry.

Readiness: design authority has been updated to require whole-turn residency, Markdown-safe render chunks for huge turns, and measured chunk heights for row interior spacers. The fixed `96px` block-window path is an invalidated approach and must not remain in control of visible omission.

Resumable milestone: Phase 3 is in progress.

# Phase 1: Replace Fragile Block Window Model (finished)

Tasks:

- Replace the current fixed-height block-window contract with a presentation model that can represent stable render chunks inside one resident transcript row.
- Split large resident turns at Markdown-safe presentation boundaries, preserving backend turn identity, item order, copy-source spans, media ownership, code-panel ids, context-menu targeting, and selection semantics.
- Keep normal turns on the whole-row render path.
- Disable or remove any path where `TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX` or another fixed pixel estimate decides which content is visible or how tall skipped visible-adjacent spacers are.
- Define chunk-cost thresholds from presentation facts such as text bytes, Markdown block count, line count, media count, code-panel count, and item count. These thresholds may trigger chunking and bound chunk source size only.

Edge cases:

- Huge assistant text made of many short paragraphs.
- Huge fenced code block and huge single-line code block.
- Mixed reasoning, commentary, final answer, user fragments, and media runs in one backend turn.
- Markdown constructs that must not be split in the middle of a semantic block or copy-source span.

Automated tests:

- Add focused `transcript_presentation` coverage for deterministic chunk identity, stable order, source-span preservation, and markdown-safe split boundaries.
- Add large-turn tests where many short blocks trigger chunking but do not use fixed pixel estimates for the visible window.
- Add source-guard coverage in `conversation_shell_source` or an equivalent source test proving fixed per-block pixel estimates no longer control visible omission or spacer height.

Verification:

- `cargo nextest run -p beryl-app --test transcript_presentation --test conversation_shell_source` passes.
- A broader `cargo nextest run -p beryl-app transcript_presentation conversation_shell_source` currently fails before running the focused tests because unrelated `gui_control_dynamic_tools` initializers are missing `UiStateSnapshot` fields `markdown_cache` and `pending_activation`.

Resumable milestone: chunk models exist, normal rows still render whole, large rows can be identified and split without relying on guessed pixel geometry, and the fixed pixel block-window path no longer controls visible omission.

# Phase 2: Add Measured Chunk Geometry (finished)

Tasks:

- Add chunk measurement records keyed by row identity, row presentation revision, chunk identity, transcript width, theme/font revision, and layout-affecting display state.
- Build a row-interior geometry helper that maps row scroll offset and viewport height to chunk ranges using measured cumulative chunk heights.
- Render chunks around the current row anchor until measured chunk heights cover the viewport plus bounded render overscan, or until the row boundary is reached.
- Use measured heights for top and bottom spacers when measured offscreen chunks are skipped.
- For unknown geometry, over-render additional chunks, retain prior measured geometry until explicit invalidation, or use an explicit bounded large-turn fallback. Do not insert blank guessed spacers for possibly visible content.
- Preserve semantic scroll anchors when chunk heights change after measurement or invalidation.

Edge cases:

- Short chunks where under-rendering previously created a blank lower viewport.
- Mixed very short and very tall chunks.
- Unknown chunk heights at the top, middle, and bottom of a large row.
- Width, theme, font, media readiness, code-panel state, and display-state changes that invalidate chunk measurements.

Automated tests:

- Add pure geometry tests with artificial measured chunk heights for range selection, prefix offsets, spacer heights, overscan, and boundary clamping.
- Add tests proving unknown geometry chooses over-render or explicit fallback rather than blank spacer geometry.
- Add invalidation tests for width, theme/font revision, media readiness, and display-state changes.
- Add scroll-anchor tests proving row offsets reconcile when measured chunk heights change.

Verification:

- `cargo nextest run -p beryl-app --test transcript_presentation --test conversation_shell_source` passes with 135 tests.
- `git diff --check` passes.
- `rg -n "TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX|block_presentation\(|requires_block_split|TranscriptRowBlockPresentation|TranscriptRowBlockUnit" crates/beryl-app/src crates/beryl-app/tests doc/features/transcript/design.md doc/plan.md` finds only the plan task text and negative source guards.

Resumable milestone: large rows render from measured chunk geometry, and the previous blank-bottom failure shape is covered by tests.

# Phase 3: Integrate Residency, Diagnostics, And Render Metrics (wip)

Tasks:

- Keep transcript residency admission and release at whole-turn granularity while allowing the renderer to chunk presentation for an admitted turn.
- Ensure selected-thread activation loads the default tail turn first, stages coherent row presentation, obtains activation-time row measurement before publication, then expands the resident window until the policy margin is satisfied or budget limits are hit.
- Preserve oversized-turn fallback behavior only for turns that exceed the resident turn-data budget, not merely for turns that exceed frame render budget.
- Extend bounded diagnostics with large-turn chunk counts, measured versus unknown chunk geometry counts, rendered chunk counts, and chunk fallback reasons without logging transcript content.
- Keep render-frame metrics content-free and bounded around snapshot construction, chunk-window computation, and render-adjacent pruning.

Edge cases:

- A single huge resident turn already satisfies the viewport margin.
- Several small turns must be loaded after the default turn to satisfy the viewport margin.
- A huge turn fits resident data budget but exceeds frame render budget.
- A turn exceeds resident data budget and must render the explicit oversized fallback.
- Resident-window release keeps enough measured row or chunk geometry to avoid scroll jumps.

Automated tests:

- Add residency planner tests proving whole-turn measured heights, not chunk counts or source bytes, decide whether the viewport-height margin is satisfied.
- Add shell/source guards proving renderer, prepaint, deferred, media preload, status-line, and scrollbar callbacks do not independently schedule resident loads.
- Add diagnostics tests proving new chunk counters are present, bounded, and content-free.
- Add integration coverage for activation at tail with one huge turn and with multiple small turns.

Resumable milestone: residency and rendering budgets are separated, diagnostics expose chunk behavior, and activation/window expansion follow measured whole-turn rows.

Blocked issue:

- Selected-thread activation does not currently have a technical path to obtain activation-time visual row measurements before publication. The activation worker loads the initial full page and `initial_thread_activation_turn_admission_plan` still admits by `INITIAL_THREAD_ACTIVATION_VIEWPORT_ROWS` and `TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT`; publication waits for structural presentability only in `StagedSelectedThreadActivation::is_ready_for_publication`. Whole-row visual measurements are produced later by the GPUI virtual list through `transcript_list_state.measured_item_size`, after the transcript panel has rendered the published rows. Implementing the Phase 3 task "obtains activation-time row measurement before publication, then expands the resident window" therefore requires a design decision: introduce a staged/offscreen GPUI measurement path for activation, relax activation to publish the default tail turn first and expand immediately after first measurement, or revise the design requirement.

# Phase 4: Regression Verification And Completion Review (pending)

Tasks:

- Run focused `cargo nextest run -p beryl-app` suites for transcript presentation, large transcript presentation, transcript history, virtual trailing list, transcript scroll, and source-guard tests.
- Run broader `cargo nextest run -p beryl-app` if focused suites pass and runtime is acceptable.
- Use Beryl diagnostics or a test harness to verify the pathological many-short-paragraph turn no longer leaves the bottom half of the transcript viewport blank.
- Request a completion-review subagent after all implementation phases are finished, then address any findings through an updated plan before finalizing.

Automated regression targets:

- Many short Markdown paragraphs in one assistant turn.
- Long wrapped prose in a narrow viewport and a wide viewport.
- Huge single-line code block.
- Mixed Markdown, media placeholders, and code panels.
- Theme/font change after chunk measurement.
- Media readiness changing chunk and row heights.
- Manual scroll, tail activation, passive final runway, and detached live-scroll states interacting with a chunked huge turn.

Resumable milestone: all planned tests pass or blockers are recorded here with exact failing commands and symptoms.
