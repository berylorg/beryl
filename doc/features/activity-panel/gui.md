# Activity Panel GUI

This is a normative supplemental GUI composition file for `design.md`. It owns activity-panel slot mounts, layout relationships, and widget composition. Product behavior, activity projection, row content authority, pruning, and backend interaction remain in `design.md`.

## Activity Panel

Mount-into: main-window.activity-panel

The feature mounts one project-local [`activity panel`](../../gui/widgets/activity-panel/spec.md) as the optional bounded panel below the transcript region and above any discussion-status strip and composer. The widget owns resize geometry, fixed-height rows, bounded realization, scrolling, truncation, stable row reconciliation, tooltip anchoring, and content-free diagnostics.

The feature supplies the selected-thread revision-bound activity query identity, total logical row
count, bounded resident row pages, stable activity identities, running-first recent ordering,
status-marker state, bounded `Agent` and `Activity` row projections, and the initial top-attached
viewport policy defined in `design.md`. It answers deduplicated page requests without constructing a
whole-session activity collection.

The feature persists committed panel height as window-local Beryl-home state and supplies current minimum and maximum bounds from the conversation layout. Showing the widget takes height only from the transcript region; hiding it unmounts the slot contribution without moving the discussion-status strip, pinned composer, or global status line.
