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
