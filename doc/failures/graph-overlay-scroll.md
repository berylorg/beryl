# Graph Overlay Scroll Ownership

## Invalidated Approach

Phase 8 initially assigned vertical scrolling to nested graph-specific surfaces inside the half-height graph overlay, first at the per-column level and then at an inner graph viewport level.

## Why It Failed

Live testing showed that this split scroll ownership did not match operator expectations for the overlay as a bounded popup-like surface.

- The half-height overlay itself still clipped content when the header, warnings, and graph explorer together exceeded the visible height.
- Assigning vertical scroll only to inner graph surfaces made the outer overlay panel feel fixed and clipped rather than like one coherent scrollable panel.
- Keeping vertical scroll inside the graph columns also conflicted with the desired interaction model once the operator clarified that the whole overlay panel should scroll vertically.

## Course Adjustment

The graph overlay panel itself now owns vertical scrolling for overflow within its half-height bounds.

- The graph columns container continues to own horizontal scrolling for explorer depth.
- Graph columns no longer own vertical scrolling in V1.
- `doc/ui.md` was updated to reflect this panel-level vertical scroll contract.

## Later Invalidation

Subsequent live testing invalidated the panel-level vertical scroll course adjustment as the steady-state interaction model for the explorer overlay.

- Scrolling one early explorer column downward and then selecting a node hid newly opened later columns below that shared overlay scroll position.
- The operator wanted each explorer column to preserve its own independent vertical position so the next selected column appears immediately without resetting or fighting a global overlay scroll offset.
- A shared overlay-level vertical scroll also made the whole popup behave like one long document rather than a row of bounded explorer panes.

## Current Adjustment

The graph overlay popup remains fixed-height and non-vertically-scrollable, while scroll ownership moves back to the individual explorer columns.

- The graph columns container continues to own horizontal scrolling for explorer depth.
- Each graph column owns vertical scrolling for its own rows beneath a fixed column header.
- `doc/ui.md` and `doc/product-features.md` now reflect this per-column vertical scroll contract.
