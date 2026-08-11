# Name

Canonical name: table panel

Sometimes known as: data table panel, transcript table

# Purpose

Presents large tabular transcript content through bounded row-and-column realization, stable range selection, copy affordances, and independently routed scrolling.

# References

Contracts:

- scroll-ownership

Widgets:

- command button
- scrollbar

# Anatomy

The table panel consists of a root frame, optional header strip, optional title or summary, optional command controls, grid viewport, realized grid window, optional row headers, optional column headers, body cells, header cells, local range-state layer, and vertical and horizontal scrollbar affordances. Every header cell uses both the base cell part and the header-cell part, so it retains ordinary cell geometry while receiving header-specific styling.

The owner supplies table identity, revision, row and column counts when known, stable row and column identities, resident cell ranges, cell presentation, copy payloads, range-demand outcomes, and local fallback meaning.

The widget owns visible-range geometry, bounded two-axis realization, focus and selection mechanics, nested scroll routing, and content-free diagnostics. It does not load storage, parse Markdown, derive the table model, retain resource slices, or decide resource budgets.

# Look

The widget reads as one bounded inset panel distinct from surrounding prose. Column headers and optional row headers remain visually distinguishable from body cells without changing grid geometry as focus or selection moves.

Grid lines, alternating-row treatment, selection, focus, pending ranges, and local fallback treatment preserve row and column measurements. Truncated visible cell content uses owner-supplied accessibility text and copy data rather than treating the paint truncation as source content.

# States

The widget supports ready, empty, focused, selected-for-scroll, horizontally overflowing, vertically overflowing, range-pending, range-rejected, range-unavailable, revision-changing, selection-active, selection-invalidated, copy-pending, copy-failed, and local-fallback states.

Cells support normal, focused, selected, truncated, pending, unavailable, and fallback states. Headers support normal, focused, selected, pinned-leading, and partially-visible states.

Pending, rejected, unavailable, and fallback cells are local presentation states and are not selectable source content unless the owner supplies an explicit copy representation.

# Interaction

Pointer activation focuses a realized eligible cell and selects the table panel for nested pointer-wheel ownership. While selected for scroll, vertical and horizontal wheel or touchpad input over the panel scrolls only the table axis that can consume that gesture and does not co-scroll the outer transcript.

Pointer hover alone never transfers vertical wheel ownership from the transcript. Pressing Escape does not clear selected-for-scroll ownership; focus may move elsewhere through ordinary navigation or pointer activation.

Arrow keys move the focused cell through realized row and column identities. Home and End move to the first and last column of the focused row. Page Up and Page Down move by one visible row page while preserving the focused column. Owner-supplied explicit navigation may request a nonresident stable row or column and produces range demand rather than a synthetic blank grid.

Shift-modified navigation extends a rectangular selection through resident eligible cells. Selection identity is expressed by stable row and column identities, not visible indices. Selection cannot span an unrealized gap; attempting to extend through one reports demand and preserves the last coherent resident selection.

Copy requests use the owner-supplied source representation for the selected stable range and
reconstruct a contiguous platform value only after its exact logical size fits the admitted
clipboard limit. Rejection preserves the stable selection. Owner-supplied `Save…` may stream a
larger stable table range through bounded pages. The widget does not reconstruct unloaded cells or
copy painted ellipses, fallbacks, headers, or pending placeholders as authored content.

If range release, revision change, remeasurement, or fallback replacement invalidates focus or selection, the widget resolves focus to the nearest coherent realized cell and closes invalid selection. It does not pin unbounded offscreen ranges solely for focus or selection.

Scrollbar thumb dragging and lane interaction route through the table panel. Because the widget may not know full pixel geometry for unresident variable-width or variable-height ranges, scrollbar extent may be index-derived from known row and column counts rather than implying that every cell is realized.

The grid realizes only the owner-supplied visible row and column ranges plus bounded overscan. Overscan is capped independently on every axis and never expands to the complete table because of selection, focus, hover, copy, or scrollbar interaction.

Every realized row, column, and cell has stable owner-supplied identity independent of visible index. Focus, selection, measurement, copy, accessibility, and demand reporting follow those identities across viewport movement and filtering or revision changes.

Content-free diagnostics expose widget instance id, table identity, revision, known row and column counts, visible and realized row ranges, visible and realized column ranges, overscan counts on each axis, realized cell count, pending range count, scroll offsets, selected-for-scroll state, focused stable cell identity, selected stable range bounds, scrollbar presence, and fallback count. Diagnostics never include cell text, header text, copied data, file paths, or resource bytes.

# Layout

The root frame receives a bounded outer allocation from its transcript presentation record. The optional header strip remains outside the scrolling grid viewport.

The grid viewport owns independent inline and block scroll offsets. Column headers remain aligned with body columns during horizontal scrolling. Row headers remain aligned with body rows during vertical scrolling when those optional headers are configured.

Realized rows and columns are laid out only from resident range descriptors. Owner-supplied measured or estimated track sizes establish the current grid window; later coherent measurements preserve the focused cell or owner-supplied table anchor rather than resetting both scroll axes.

The horizontal and vertical scrollbar affordances occupy panel edges without changing the table's source row or column identities. The complete outer panel remains one measured transcript presentation record while its grid window scrolls internally.

Cell contents clip or truncate within their assigned track. Nested unbounded cell scroll surfaces are not supported.

Spec CSS:

```css
.table-panel {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  min-block-size: 0;
  max-inline-size: 100%;
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  overflow: hidden;
}

.table-panel__header {
  display: flex;
  align-items: center;
  flex: none;
  block-size: var(--height);
  padding-inline: var(--padding-x);
  gap: var(--gap);
  border-block-end: var(--border-width) solid var(--border-color);
}

.table-panel__viewport {
  position: relative;
  min-inline-size: 0;
  min-block-size: 0;
  max-block-size: var(--max-height);
  overflow: hidden;
}

.table-panel__grid-window {
  position: relative;
  display: grid;
  min-inline-size: max-content;
}

.table-panel__cell {
  box-sizing: border-box;
  min-inline-size: 0;
  min-block-size: var(--min-height);
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border-inline-end: var(--border-width) solid var(--border-color);
  border-block-end: var(--border-width) solid var(--border-color);
  background: var(--background);
  color: var(--foreground);
  overflow: hidden;
}

.table-panel__header-cell {
  background: var(--background);
  color: var(--foreground);
  font-weight: var(--font-weight);
}

.table-panel__cell[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.table-panel__cell[data-state~="focused"] {
  box-shadow: inset 0 0 0 var(--ring-width) var(--ring-color);
}

.table-panel__cell[data-state~="pending"],
.table-panel__cell[data-state~="unavailable"] {
  color: var(--foreground);
  opacity: var(--opacity);
}
```

# Variants

Headerless, column-header, row-and-column-header, and header-actions.

Default variant: column-header.

# UI Roles

```css
.table-panel {
  --border-width: 1px;
  --border-color: #475569;
  --radius: 7px;
  --background: #0f172a;
  --foreground: #e5e7eb;
}

.table-panel__header {
  --height: 32px;
  --padding-x: 10px;
  --gap: 8px;
  --border-width: 1px;
  --border-color: #334155;
}

.table-panel__viewport {
  --max-height: 360px;
}

.table-panel__cell {
  --min-height: 28px;
  --padding-x: 8px;
  --padding-y: 5px;
  --border-width: 1px;
  --border-color: #334155;
  --background: #0f172a;
  --foreground: #dbe4ef;
}

.table-panel__header-cell {
  --background: #111827;
  --foreground: #f1f5f9;
  --font-weight: 650;
}

.table-panel__cell[data-state~="selected"] {
  --background: #1e3a5f;
  --foreground: #f8fafc;
}

.table-panel__cell[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #38bdf8;
}

.table-panel__cell[data-state~="pending"],
.table-panel__cell[data-state~="unavailable"] {
  --foreground: #94a3b8;
  --opacity: 0.78;
}
```
