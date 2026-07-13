# Name

Canonical name: activity panel

Sometimes known as: activity viewport, backend activity panel

# Purpose

Presents bounded live and recent activity as a vertically resizable, fixed-row panel without making activity part of the conversation transcript.

# References

Contracts:

- scroll-ownership

Widgets:

- scrollbar
- tooltip

# Anatomy

The activity panel contains a root panel, a top-edge resize handle, a clipped row viewport, a realized row layer, and an external vertical scrollbar.

Each fixed-height row has an owner-supplied stable activity id, status marker, agent key, agent value, activity key, and activity value. The widget owns row geometry, truncation, stable reconciliation, and viewport realization. The owning feature supplies keys, values, ordering, lifecycle state, retention, and domain meaning.

Rows are presentation-only. They do not contain command regions, disclosure controls, output previews, or nested operational detail.

# Look

The panel reads as compact lower conversation chrome separated from the transcript and composer. Its top resize handle remains discoverable without becoming a second toolbar.

Rows remain single-line and visually stable as their status or value changes. Status markers distinguish owner-supplied running, successful, and failed states. The agent and activity keys use quieter emphasis than their values.

Long agent labels and activity values truncate inside their own regions. A tooltip may expose the complete owner-supplied accessible value while its stable row remains realized.

# States

The widget supports ready, empty, resizing, overflow, top-attached, manually scrolled, reconciling, focused resize handle, and inert states.

Rows support running, finished-ok, finished-error, unresolved label, hover, and truncated states supplied by the owning feature.

Visibility is controlled by the mounting feature. A hidden activity panel is unmounted from its integration slot rather than represented by an empty-height widget state.

# Interaction

Dragging the top-edge resize handle changes the panel's allocated height within owner-supplied minimum and maximum bounds. Pointer capture keeps the resize continuous until release or cancellation. The widget reports transient and committed heights; persistence belongs to the owning feature.

The resize handle has the accessible name `Resize activity panel`. When it has keyboard focus, Up and Down resize by the owner-supplied step, while Page Up and Page Down resize by a larger step. Home and End move to the minimum and maximum allocation. Resize commands never move the composer or status line outside the window.

The row viewport owns vertical wheel, touchpad, scrollbar, and keyboard scrolling only while it can consume movement. Boundary propagation follows `scroll-ownership`.

Rows use fixed-height virtualization. The widget realizes visible rows plus at most four overscan rows before and four after the visible range. Total retained activity count does not determine render-tree size.

Stable activity identity, not visible index, owns row reconciliation and tooltip anchoring. When sorting or lifecycle updates change row order, an attached-to-top viewport remains at the top; a manually scrolled viewport preserves the first visible stable row and its viewport offset when that row still exists.

If virtualization removes a tooltip-owning row, the tooltip closes intentionally. The widget never retains an offscreen row solely to preserve hover or tooltip geometry.

Row updates do not change fixed row height or total scroll geometry. Rows are not keyboard-focusable and do not acquire selection state.

Content-free diagnostics expose widget instance id, allocated height, resize state, total row count, realized row count, visible opaque diagnostic row-key range, overscan count, fixed row height, scroll offset, top-attachment state, reconciliation count, and tooltip-anchor presence. Diagnostic row keys are nonreversible process-local correlations. Diagnostics never include labels, activity values, commands, paths, raw backend ids, or tooltip text.

# Layout

The root fills the inline allocation supplied by `main-window.activity-panel` and uses the owner-supplied persisted block allocation clamped to the current conversation-body bounds.

The resize handle occupies the root's top edge. The viewport fills the remaining block size and clips the realized row layer. The scrollbar overlays the viewport's trailing edge without reducing row width after it appears.

Rows use one fixed block size and a stable five-part content grid: status marker, agent key, agent value, activity key, and activity value. Value regions may shrink and truncate; key regions remain content-sized, and the status marker never changes the row's block size.

Spec CSS:

```css
.activity-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  min-block-size: var(--min-height);
  max-block-size: var(--owner-max-height);
  inline-size: 100%;
  block-size: var(--owner-height);
  border-block-start: var(--border-width) solid var(--border-color);
  background: var(--background);
  color: var(--foreground);
}

.activity-panel__resize-handle {
  flex: none;
  block-size: var(--resize-handle-height);
  background: var(--background);
  cursor: ns-resize;
}

.activity-panel__viewport {
  position: relative;
  flex: 1 1 auto;
  min-block-size: 0;
  overflow: hidden;
}

.activity-panel__rows {
  position: relative;
  min-inline-size: 0;
}

.activity-panel__row {
  display: grid;
  grid-template-columns: var(--marker-size) max-content minmax(0, var(--agent-width)) max-content minmax(0, 1fr);
  align-items: center;
  box-sizing: border-box;
  block-size: var(--row-height);
  padding-inline: var(--padding-x);
  gap: var(--gap);
  white-space: nowrap;
}

.activity-panel__agent-value,
.activity-panel__activity-value {
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.activity-panel__status-marker {
  inline-size: var(--marker-size);
  block-size: var(--marker-size);
  border-radius: var(--radius);
  background: var(--background);
}

.activity-panel__key {
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
}

.activity-panel__agent-value {
  color: var(--foreground);
  font-size: var(--font-size);
}

.activity-panel__activity-value {
  color: var(--foreground);
  font-size: var(--font-size);
}

.activity-panel[data-state~="inert"] {
  opacity: var(--opacity);
}
```

`--owner-height` and `--owner-max-height` are dynamic values supplied by the containing conversation layout after applying the feature-owned persisted height and current transcript minimum.

# Variants

Default variant: compact fixed-row activity list with a top-edge resize handle.

# UI Roles

```css
.activity-panel {
  --owner-height: 144px;
  --owner-max-height: 320px;
  --min-height: 56px;
  --border-width: 1px;
  --border-color: #334155;
  --background: #0f172a;
  --foreground: #e2e8f0;
}

.activity-panel__resize-handle {
  --resize-handle-height: 6px;
  --background: #334155;
}

.activity-panel__row {
  --row-height: 28px;
  --padding-x: 10px;
  --gap: 8px;
  --marker-size: 8px;
  --agent-width: 220px;
}

.activity-panel__key {
  --foreground: #94a3b8;
  --font-size: 11px;
  --font-weight: 500;
}

.activity-panel__agent-value {
  --foreground: #cbd5e1;
  --font-size: 12px;
}

.activity-panel__activity-value {
  --foreground: #f1f5f9;
  --font-size: 12px;
}

.activity-panel__status-marker {
  --marker-size: 8px;
  --radius: 999px;
  --background: #64748b;
}

.activity-panel__status-marker[data-state~="running"] {
  --background: #38bdf8;
}

.activity-panel__status-marker[data-state~="finished-ok"] {
  --background: #22c55e;
}

.activity-panel__status-marker[data-state~="finished-error"] {
  --background: #ef4444;
}

.activity-panel[data-state~="inert"] {
  --opacity: 0.55;
}
```
