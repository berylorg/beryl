---
name: gpui-scroll-surfaces
description: Use when creating, reviewing, or optimizing GPUI scrollable lists, tables, trees, sidebars, pickers, transcript rows, settings rows, activity logs, or any repeated row surface. Ensures an explicit bounded rendering strategy, virtualization/windowing decision, stable identity, focus/popover preservation, and scroll performance diagnostics before or during implementation.
---

# GPUI Scroll Surfaces

## Purpose

Build GPUI scrollable row surfaces with an explicit rendering strategy before live testing discovers sluggish scrolling.

Prefer bounded rendering for any list whose row count can grow, whose rows contain text layout or controls, or whose debug-build scroll performance matters.

## First Decision

Before implementing a GPUI scroll surface, classify it:

1. Static full render.
   Use only when the maximum row count is statically small and documented by the surrounding model contract.

2. Cached full render.
   Use only when rows are cheap, the count is modest, and model construction or style lookup is the main cost rather than render tree size.

3. Virtualized/windowed render.
   Use by default when row count can grow, rows wrap text, rows contain controls, rows have hover/active styling, rows allocate strings or closures, or the surface is expected to scroll smoothly in debug builds.

Do not implement a GPUI scroll surface as `overflow_y_scroll().children(items.iter().map(render_row))` unless the row count is statically bounded and the reason is explicit.

## Preferred Shape

Prefer fixed-height rows when the surface is a selector, navigator, table, menu, or log.

Use variable-height virtualization only when row height must genuinely depend on content. If variable heights are required, plan measurement, cache invalidation, and scroll-position reconciliation before writing the row renderer.

For fixed-height virtualized lists:

- Store total item count separately from rendered item count.
- Compute visible row range from scroll offset, viewport height, row height, and overscan.
- Render only visible rows plus bounded overscan.
- Preserve total scroll extent with top and bottom spacer elements or the GPUI list primitive chosen for the project.
- Keep row height stable across hover, selected, focused, validation, and loading states.

## Contracts To Preserve

Virtualization must preserve behavior, not just speed.

Preserve:

- stable row ids and event targets
- selected row state and selected-row reveal behavior
- scroll position across model refreshes
- focus and keyboard traversal
- text input retained state
- popover, context menu, color picker, tooltip, and dropdown anchors
- row action dispatch for visible and newly-visible rows
- hover, active, pressed, disabled, validation, and modified states
- scrollbar semantics and pointer/wheel behavior
- accessibility labels or equivalent descriptive text if the surface provides them

If any contract cannot be preserved cleanly, stop and redesign the surface instead of adding a workaround.

## Render Hot-Path Checklist

Keep row render functions cheap.

Avoid:

- whole-model scans from inside `render_row`
- resolver calls from inside row render
- parsing colors, fonts, markdown, or syntax highlighting during render
- formatting stable ids or labels repeatedly when they can be precomputed
- rebuilding retained input entities for offscreen rows
- allocating large strings or vectors per row per frame
- attaching expensive closures that capture large state
- wrapping long text in narrow selector rows unless row height and layout cost are intentional

Prefer:

- precomputed row presentation data
- stable ids from the model
- small value objects for row style
- app-neutral list primitives in reusable crates
- host-owned domain semantics outside reusable UI crates

## Diagnostics

Add or use diagnostics before guessing about scroll slowness.

Record bounded, content-free values such as:

- surface id
- total row count
- rendered row count
- visible row range
- overscan count
- row height strategy
- render/model-sync timing when available
- cache hit/miss counts for expensive presentation data
- focus or popover anchor row id when relevant

Do not log private row contents, file paths, user text, or secrets.

## Verification

For non-trivial scroll surfaces, verify:

- rendered row count remains bounded while total row count grows
- scrolling does not rebuild offscreen retained inputs
- selection and row actions target the intended stable ids
- focus survives scroll away and back when the UI contract requires it
- popovers remain anchored or close intentionally when their row leaves the viewport
- resize, theme change, font-size change, and model refresh reconcile visible range correctly
- debug-build live scrolling is acceptable for the expected row count

When a list is intentionally not virtualized, add a test or source assertion that documents the bounded maximum row count or the reason full rendering is acceptable.
