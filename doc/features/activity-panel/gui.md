# Activity Panel GUI

This is a normative supplemental GUI composition file for `design.md`. It owns activity-panel slot mounts, layout relationships, and widget composition. Product behavior, activity projection, row content authority, pruning, and backend interaction remain in `design.md`.

## Activity Panel

Mount-into: main-window.activity-panel

The activity panel mounts as the optional bounded panel between the transcript region and the composer panel. When visible, it takes height from the transcript region and preserves the pinned composer and status line.

The panel has a draggable top border for vertical resize. Resize state persists as workspace-scoped GUI-local state.

The panel body is a compact single-line row list. Rows do not wrap; long agent labels and activity values truncate within available width.

When visible rows exceed panel height, the panel owns vertical scrolling and uses bounded viewport rendering with small overscan. Otherwise it does not expose scrolling.
