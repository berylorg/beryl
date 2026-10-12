# Transcript Measurement Publication

## Scope And Invalidated Approach

Root-plan phase 753 attempted to mount natural record measurement and ordinary input through
the existing resident frame controller and panel at baseline `1052e5fc` on 2026-10-10.
The proposed canvas pass cleared old heights, measured provisional realized windows, then
published the controller's final frame. A correction measured the saved anchor and adjacent
records before rebasing and limited queued wheel displacement to one viewport per render.
Independent semantic review invalidated both approaches. All provisional source and test
changes were withdrawn; no measured viewport behavior was accepted.

## Decisive Evidence

- Clearing heights before preview lets `RealizedFrameScrollController::rebase_anchor` cross
  a tall, partially visible anchor using the default row extent. Measuring the guessed
  destination cannot recover the skipped record's actual displacement.
- Measuring the saved anchor first fixes that case but does not bound shrink reconciliation.
  An anchor previously 11,000 pixels tall at viewport y=-10,000 may shrink to 36 pixels.
  With 500 following admitted records, the rejected synchronous traversal visits roughly 278
  intervening rows before reaching the viewport. Capping new wheel input does not limit this
  zero-input work. Dropping elements after measuring them still allows the provisional
  height collection and one render's layout work to scale with the resident collection.
- Final clamping changes the needed measured window. With ten 36-pixel rows, viewport 200,
  overscan 100 and detached anchor index 4/y=-16, a forward 200-pixel request initially
  measures indices 4 through 9. Trailing clamping moves the window into earlier records.
  Publishing immediately assigns default heights and `MeasuredRecord` facts to final rows
  which were never measured. The final clamped window needs qualification before publication.
- A last anchored row shrinking below its saved negative offset can produce an empty frame
  with no clamp because `rebase_anchor` handles a trailing edge only for forward manual input.
  This was reproduced against the unchanged controller with a temporary focused nextest case:
  21 records, viewport 640, overscan 320, default extent 36; measure the last record at 11,000;
  activate tail; scroll backward 9,360 to put that record at y=-1,000; replace its observation
  with height 36; realize with zero delta. Both visible range and realized records are empty,
  and clamp is absent. This is failure qualification, not target acceptance.

The temporary reproduction passed under nextest run
`04ff6ffa-fcba-4cc7-8665-4fb1aac204b3` (one case, thirteen skipped), using
`cargo +stable --config .cargo/local.toml nextest run -p beryl-app --test syndic_transcript_realized_frame -E 'test(measured_shrink_source_qualification)' --test-threads 1`.
Build jobs were one, development/test debug information zero and incremental compilation disabled.
The temporary case was removed after preserving its fixture and observed result here; it asserted
the existing failure rather than the target contract.

The independent reviewer also identified ordinary click focus, environment-wide transient
geometry invalidation, publication of provisional measurement facts, and queued input crossing
selection boundaries. A blanket input reset on coherent publication additionally loses exact
input during same-view refresh. These belong to the complete viewport acceptance boundary.

## Evidence Limits

The provisional App check passed and six focused integration targets passed 75/75 cases;
those checks did not establish the failed geometry and publication guarantees. The expanded
GPUI case reached variable-height layout but retained an invalid fixture assumption that a
tall tail row must fit wholly inside the viewport. The ordinary native fixture compiled and
launched best-effort minimized, then aborted because it drew the shell while the root entity
was already being updated. Native input and paint qualification were not obtained. Future
native qualification must separate window drawing/input dispatch from root entity updates.
The exact failed native fixture directory was reclaimed after process exit.

## Accepted Clean Correction

Resolve bounded measurement preparation and coherent publication before mounting the viewport:

- The existing transcript host owns one current preparation keyed by host/view, activation,
  presentation/record revisions and the measurement environment. The renderer supplies bounded
  natural-layout observations for that exact preparation; no candidate measurement escapes
  as a committed demand fact.
- Give refill and shrink traversal an explicit per-frame work limit. Preserve traversal progress
  and remaining exact displacement as bounded state, releasing crossed row elements and
  measurements instead of accumulating a resident-collection-sized provisional height collection.
- Keep the previous coherent frame and its associated selected-content metadata until the
  replacement measured geometry is ready. Initial selected content, measured tail placement
  and the corresponding metadata must still publish together. This requires qualification at
  the actual selected attachment/publication consumer, beyond a panel-only adapter.
- Qualify the final clamped frame, then publish its geometry, anchor and demand facts once.
  Preserve queued exact input across coherent same-view refresh and retire it on changed
  selection authority. Invalidate transient geometry and release all superseded preparation
  state on environment change, replacement, cancellation and close.

Operator approved this correction on 2026-10-10. The owning
[renderer architecture](../systems/transcript-presentation/renderer-architecture.md#measured-frame-preparation)
and [shell boundary](../systems/transcript-presentation/shell-boundary.md#measured-publication-gate)
now determine the bounded preparation/publication lifecycle; this failure record remains evidence.
The root plan releases the technical hold and requires complete production qualification.
Resource delivery, Markdown, nested widgets, streaming history publication and complete transcript mounting
remain separate outcomes.

## Correction Disposition

The accepted bounded preparation is implemented and qualified by the
[viewport acceptance](../audits/transcript-mounting-readiness.md#measured-viewport-acceptance).
The zero-input shrink now refills and clamps measured rows across bounded preparation steps;
initial publication waits for its complete measured frame and corresponding shell metadata.

Review found that the ordinary live-selection helper cannot validate recovered composers before
their recovery fence is released. The corrected gate checks exact selected identity and retained
recovery ticket while fenced, completes measured publication before reopening canonical admission,
and propagates rejection instead of equating candidate absence with successful publication.
Cancelled or stale candidates release their payload immediately and retain only a failure diagnostic.
Current appearance is synchronized at asynchronous consumers as well as normal root rendering.

Native paint qualification also cannot inspect GPUI's next-frame buffer after drawing swaps and
clears it. The test now counts transcript glyphs inside the panel's actual paint callback. The
earlier failed buffer assertion supplies no paint evidence; the corrected native case passes.
