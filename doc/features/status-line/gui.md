# Status Line GUI

This is a normative supplemental GUI composition file for `design.md`. It owns status-line slot mounts, layout relationships, and widget composition. Product behavior, metadata authority, popup gating, compaction, stop semantics, and failure reporting remain in `design.md`.

## Status Line Strip

Mount-into: main-window.status-line

The feature mounts one bundled `segmented status bar` as the fixed bottom strip below the composer. This is a feature configuration of the bundled widget rather than a Beryl-specific status widget: the built-in owns strip, segment, divider, focus, open, disabled, and truncation mechanics, while `design.md` owns values, availability, and operations.

The feature configures three stable left-to-right segment identities:

- Model and reasoning.
- Context space and rate-limit status.
- Turn state plus transcript-view turn position.

Model/reasoning and context are action-menu segments when their exact operations are available. The turn segment is an action-menu segment only when an exact interruptible operation is known; its `View` subsegment remains passive and never becomes a separate activation target.

Segments retain identity and geometry while displayed values change, including while an anchored popup is open. Unknown, unavailable, compacting, working, success, error, and `View` values update inside the existing segment geometry. Truncated exact values use the bundled `tooltip` without becoming a second status source.

When an operation is unavailable, its segment remains passive or visibly disabled according to `design.md`. Any visible disabled command-capable segment uses `disabled-command-tooltip` with the closest exact gate; it never opens an empty popup.

## Status Operation Popups

Mount-into: main-window.overlays

Each status operation popup uses the bundled `anchored context menu`, whose row presentation follows
the bundled `context menu`. The model/reasoning popup configures the `virtualized-collection`
variant with an exact backend query identity, bounded resident `model/list` cursor pages, stable
option identities, continuation state, logical focus, selected-row reveal, and bounded realization;
it never supplies the complete caller-unbounded model collection. The context popup configures the
static bounded `Compact` command row. The turn popup configures static bounded `Soft stop` and
`Hard stop` rows, with `Hard stop` using a full-width bundled `hold-to-confirm button` and the
three-second feature-owned hold duration.

The feature supplies row labels, selected values, supported combinations, exact target identity, command effects, in-flight state, failures, and closest disabled reasons. Built-in widgets own menu bounds, row focus, dismissal, hold progress and cancellation, duplicate-activation suppression, tooltip presentation, and focus return.

Popups remain anchored to the same stable segment instance while its displayed value changes and remain within the OS window bounds. If the exact selected thread, active-turn target, or segment availability changes incompatibly, the feature closes the popup intentionally and focus returns to its segment. Closing a popup does not change transcript selection, draft content, or unrelated status segments.
