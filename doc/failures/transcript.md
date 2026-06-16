# Transcript Scroll Anchor Geometry

## Incomplete Narrative Measurement

- Scope: live transcript autoscroll final-start anchoring.
- Invalid assumption: final-answer anchor geometry could measure only user prompt blocks and assistant message markdown blocks.
- Evidence: the rendered turn row also includes reasoning markdown blocks and generated media runs through `render_turn_card`, `render_item_units`, and media flushing.
- Why it failed: visible pre-final reasoning or media can shift the rendered final answer lower than the measured final offset, so a final-start scroll can land above the actual final response.
- Course correction: the live-scroll measurement pass must mirror the rendered narrative sequence closely enough for all visible pre-final blocks, including split markdown/media runs, reasoning markdown, and generated media placeholders.
- Affected tests: keep focused coverage for final-start placement and source guards for the measured block kinds.

## Live Anchor Viewport Resizes

- Scope: live transcript prompt and final-read anchoring.
- Invalid assumption: virtual response runway computed from the previous transcript panel height would remain sufficient for the next list layout.
- Evidence: live testing showed a submitted prompt initially anchored at the top while the activity panel was visible, then older transcript text appeared above the prompt after turn completion removed the activity panel and the transcript viewport grew.
- Why it failed: if virtual runway is sized for a shorter previous viewport, the current taller viewport may not have enough content below the live anchor to fill the list. The virtual list then walks upward into previous rows, moving the live anchor down without any explicit autoscroll command.
- Course correction: live prompt and final-read anchors use anchor-offset preservation and may extend virtual trailing slack during layout when the current viewport is taller than the runway computed before layout.
- Affected tests: keep virtual trailing-list coverage for preserved anchors when the viewport grows.

## Unkeyed Applied Defers

- Scope: live transcript prompt and final-start state transitions.
- Invalid assumption: deferred callbacks that mark prompt-reread or final-start scroll effects as applied could safely check only the current phase kind.
- Evidence: completion review found that a queued turn or changed final anchor could replace the live-scroll phase before an older deferred callback ran.
- Why it failed: an old callback could mark a newer prompt or final anchor as applied even though that newer anchor's scroll effect had not run.
- Course correction: deferred applied callbacks carry the exact anchor they applied, and the live-scroll state machine ignores the callback if the current phase no longer matches that anchor.
- Affected tests: keep stale prompt-reread and stale final-start defer coverage in `transcript_live_scroll`.

## Detached Manual Runway Removal

- Scope: live transcript manual scrolling after a new user-authored turn.
- Invalid assumption: detaching live autoscroll on user scroll could safely remove the virtual response runway.
- Evidence: live testing showed that after a turn settled, the first manual scroll removed the virtual space below the latest turn and made the prompt-at-top reading position unreachable.
- Why it failed: the runway is not only an automatic-scroll implementation detail. It also defines a manual scroll range the UI has made meaningful by initially anchoring the latest prompt and final response.
- Course correction: detached manual state retains bounded virtual runway for the latest user-authored reading cycle, but exposes it only as non-scrolling geometry so automatic prompt, commentary, final, and completion anchoring remain detached.
- Affected tests: keep live-scroll coverage for detached manual runway and existing-history reset behavior.

## Prompt-Only Detached Runway

- Scope: live transcript manual scrolling after final-answer anchoring.
- Invalid assumption: retaining prompt-reread runway after manual detachment was enough to preserve useful manual scroll positions.
- Evidence: live testing showed the prompt-at-top position was reachable after manual scroll, but the final-answer-start-at-top position created by automatic final anchoring was no longer reachable when the final response was shorter than the viewport.
- Why it failed: once final-answer content exists, the strongest reading anchor is the final-answer start, not the prompt. Prompt-only runway forces the user back to a mixed prompt-plus-final view and prevents returning to the final-only reading position.
- Course correction: detached manual state prefers bounded final-start runway after stable final-answer content exists, with prompt runway retained only as the pre-final fallback. The final-start runway is non-scrolling and must not reissue final anchoring or follow final growth.
- Affected tests: keep live-scroll coverage for detached manual final runway, prompt fallback before final, same-turn final detection after detach, and existing-history reset behavior.

## Commentary Arrival Collapsing Prompt Runway

- Scope: live transcript prompt-reread behavior while assistant commentary starts streaming.
- Invalid assumption: detecting the first commentary item could immediately replace prompt-reread state with commentary-follow state, because the follow scroll helper would avoid issuing a scroll when the commentary still fit.
- Evidence: live testing showed the submitted prompt initially anchored at the top, then moved downward as soon as narrative commentary appeared.
- Why it failed: even without an explicit `scroll_to`, replacing prompt-reread with commentary-follow removed the virtual runway that made the prompt-top position representable. The virtual list then clamped to real content and pulled older transcript geometry into view.
- Course correction: commentary detection during prompt-reread records only a pending follow target. The render pass keeps prompt runway until measured commentary growth actually needs a lower scroll offset; only that overflow measurement promotes the state to commentary-follow.
- Affected tests: keep live-scroll coverage for pending commentary, stale pending-commentary defers, final-start override, and source guards for the pending-commentary render path.

## Raw Final-Start Top Clipping

- Scope: live transcript final-answer anchoring and retained manual runway.
- Invalid assumption: scrolling exactly to the semantic final-answer block top is always visually safe.
- Evidence: live testing showed the first final-response line partially clipped at the top edge, and the manual scroll stop could land at the same clipped position. A one-pixel guard still clipped in the real renderer when passive runway was present after fresh-start history load. A fixed larger guard improved one font configuration but was not reliable across transcript font-size or theme changes.
- Why it failed: a raw top-edge anchor, a purely mathematical one-pixel guard, or an arbitrary fixed visible guard leaves the placement coupled to one renderer/font configuration. The guard must come from the rendered final-answer text metrics so the protected top edge scales with the line that is being anchored.
- Course correction: final-answer start placement derives its visible top-edge paint guard from the measured first rendered final-answer line height, with a small minimum floor, and includes that guarded offset in the virtual runway calculation. Automatic and manual final-start positions remain representable without clipping and without extra scroll range beyond the guarded stop.
- Affected tests: keep final-start placement coverage for guarded scroll offset, line-height-scaled guard behavior, exact runway fill, virtual-tail max-stop coverage, and source guards for the metric-derived paint-guard rule.

## Loaded History Lost Final Runway

- Scope: selected-thread activation after restarting Beryl.
- Invalid assumption: loaded history should never synthesize any trailing runway because live-turn runway belongs only to accepted new turns and active streaming.
- Evidence: live testing on a freshly started Beryl instance showed the transcript could not scroll past the last line of the latest completed turn, making the final-answer-start-at-top reading position unreachable. Old diagnostics also showed the selected thread could first present the latest turn as a skeleton and only later apply full final-answer text.
- Why it failed: retained trailing runway is also a manual reading affordance after restart, not only a live autoscroll artifact. Opening at the real history tail is correct, but the prepared resident history window still needs passive final-answer slack when its latest resident turn already has stable final content.
- Course correction: selected-thread activation may seed a passive final-answer runway for the latest resident final answer during the same atomic activation transaction that publishes the prepared resident transcript window. It must not depend on skeleton rows, late transcript-detail replacement, live final-start autoscroll, final growth following, or deferred renderer callbacks.
- Affected tests: keep live-scroll state coverage for passive resident-history final runway, tail activation, detached-tail state, and source guards proving activation seeds passive runway before the list first renders without scheduling scroll callbacks.

## Assistant Geometry Reused Prompt Width

- Scope: transcript anchor geometry for assistant narrative before final-answer anchoring.
- Invalid assumption: assistant commentary, reasoning, and final-answer Markdown geometry could reuse the user prompt text width.
- Evidence: completion review found that prompt width subtracts prompt-card border and padding, while assistant Markdown renders directly in the row narrative content. Wrapped pre-final assistant content could therefore be measured taller than it renders.
- Why it failed: final-start offsets depend on the rendered height of all narrative blocks above the final answer. Using the narrower prompt-card width for assistant blocks can add phantom wrapped lines above the final answer, making final-start placement land too low or promoting commentary following before real overflow.
- Course correction: anchor geometry now measures prompt Markdown at prompt-card content width, assistant Markdown at row narrative content width, and each role with its matching text metrics.
- Affected tests: keep source guards and focused width coverage proving prompt and assistant narrative measurement widths stay distinct.

## Manual Scroll Into Non-Resident History

- Scope: active-thread manual scrollback through lazy-loaded historical transcript rows.
- Invalid assumption: proving Turn View was passive and testing the live-scroll state machine in isolation was enough to protect manual scrollback while historical details load.
- Evidence: live testing on June 10, 2026 showed that scrolling upward in an active thread exposed repeated `Loading transcript details...` rows and immediately returned the viewport to the active/current turn. Content-free diagnostics showed 22 presentation rows, 21 missing-detail skeleton turns, repeated successful detail requests, and the visible range returning to the active tail row.
- Why it failed: the missing integration case combined manual scroll detachment, active live-turn anchoring, render-driven detail scheduling, detail apply/release, list measurement, and retention. If any path reasserted a live/tail viewport or released visible detail, loaded details churned back to placeholders and the user could not inspect history.
- Course correction: the historical scroll/detail scheduler fix is superseded by the resident-window architecture. Production scrolling must clamp at resident full-detail boundaries, let the transcript residency controller own loading/release/extension, and never expose skeleton or synthetic loading rows as scrollable transcript content. Regression coverage must exercise shell/list/residency/live-scroll integration and prove wheel intent detaches live anchoring before range-dependent residency work.
- Affected plan: future transcript rewrite phases must derive from `doc/features/transcript/design.md` residency policy, not from the old skeleton detail-loading scheduler.

## Fixed-Height Large-Turn Block Window

- Scope: transcript rendering for very large resident turns.
- Invalid assumption: a fixed per-block pixel estimate could safely choose which blocks of a large turn to render and how tall skipped block spacers should be.
- Evidence: after the turn-loading and rendering rewrite, live behavior showed the transcript viewport with its lower portion blank and text disappearing around the middle of the viewport. Source inspection found `TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX` driving `TranscriptRowBlockPresentation::render_window`, and `render_turn_card_block_window` inserting blank top and bottom spacers from that estimate.
- Why it failed: Markdown blocks have variable rendered height based on width, font, theme, wrapping, code panels, media placeholders, and block margins. When real blocks are shorter than the fixed estimate, the renderer under-renders visible content and starts the blank spacer inside the viewport.
- Course correction: whole-turn residency must remain separate from frame render budgeting. Large turns may be split into Markdown-safe render chunks, and checked-in empirical render-cost weights may detect, size, or reject chunk rendering. Beryl must not repair huge turns by replacing unrendered chunk ranges with guessed or cumulative spacer geometry. Huge turns stream by semantic turn/chunk anchor; only rendered chunk measurements may influence current viewport fill and local anchor placement.
- Affected design and tests: `doc/features/transcript/design.md` requires semantic streamed chunk presentation, render-cost admission that is not visual-height estimation, and selected-thread activation that publishes a coherent tail seed before measured or streamed controller-owned expansion. Regression coverage must include a large turn made of many short Markdown paragraphs, semantic chunk navigation, width/theme invalidation, render-budget fallback for resident content that cannot be safely rendered, and source guards preventing fixed per-block estimates or measured spacer geometry from controlling visible omission.

## Continuous Huge-Turn Pixel Geometry

- Scope: transcript navigation and rendering for very large resident turns.
- Invalid assumption: preserving a continuous pixel coordinate system inside one huge turn was necessary enough to justify measured top/bottom spacers, cumulative chunk offsets, and transcript scrollbar geometry.
- Evidence: design review showed the user-visible goals are responsive rendering, memory bounds, smooth semantic navigation, and a full viewport when source data exists. The visual transcript scrollbar, exact page-scroll geometry, and selection through unrendered chunks are acceptable to remove for this rewrite.
- Why it failed: continuous huge-turn geometry keeps correctness coupled to offscreen measurement and pushes the implementation back toward fragile spacer behavior. It also preserves old scrollbar-driven behavior that the new core does not need.
- Course correction: streamed huge-turn mode abandons continuous pixel geometry. It stores a semantic anchor shaped by turn identity, chunk identity, local anchor offset when needed, rendered chunk range, and navigation direction. The renderer fills adjacent chunks around that anchor until the viewport plus overscan is full or an explicit fallback/boundary is reached. The transcript region renders no visual scrollbar, and selection/copy in streamed huge-turn mode is limited to rendered chunks.
- Affected design and tests: `doc/features/transcript/design.md`, `doc/ui.md`, `crates/beryl-app/doc/design.md`, and `doc/plan.md` require the semantic streaming path. Tests must guard against transcript scrollbar rendering, unrendered-chunk spacers, cumulative chunk scroll offsets, and render-anyway controls in this phase.

## Streamed Chunk Measurement During List Prepaint

- Scope: streamed transcript chunk measurement and virtual-list row invalidation.
- Invalid assumption: a rendered chunk `on_children_prepainted` callback could immediately record the chunk height and invalidate the owning virtual-list row.
- Evidence: live testing on June 13, 2026 scrolled upward through a large selected thread and panicked at `crates/beryl-app/src/shell/virtual_list/state.rs:138` with `RefCell already borrowed`.
- Why it failed: `List::prepaint` holds the virtual-list `StateInner` `RefCell` mutably while prepainting child row elements. The chunk measurement callback re-entered `ListState::invalidate_item_measurement` for the same list before that borrow was released.
- Course correction: streamed chunk measurement callbacks defer measurement application until after child prepaint, then changed measurements may invalidate the row for the next frame.
- Affected tests: keep a source guard proving `render_measured_chunk` uses `window.defer` before calling `record_transcript_row_chunk_measurement`.

## Mechanical Archive Before Shell Boundary Replacement

- Scope: Syndic-to-renderer architectural rework Phase 2.
- Invalid assumption: legacy transcript source could be moved under `doc/rework/syndic-to-renderer/old-crates/` after adding a small compile stub.
- Evidence: source audit found direct dependencies from `ShellView`, `ConversationSurfaceState`, selected-thread activation, diagnostics, render theme construction, and integration tests to legacy transcript module names and data types.
- Why it failed: preserving the old names as no-op shells would be an adapter-shaped compatibility layer and would keep obsolete models in the live architecture.
- Course correction: first introduce a new shell-facing `syndic_transcript` surface and move live callers to target-state host, presentation, residency, and diagnostic contracts. Archive old transcript source only after the live module tree no longer imports old transcript modules.
- Affected plan: `doc/plan.md` Phase 2 remains `wip` and `doc/rework/syndic-to-renderer/REWORK.md` tracks the replacement boundary and obsolete-test retirement before source archiving.
