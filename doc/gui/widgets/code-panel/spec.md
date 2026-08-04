# Name

Canonical name: code panel

Sometimes known as: code block panel, code viewer

# Purpose

Presents code-like plain text with optional syntax highlighting, wrapping controls, copy behavior, and bounded scrolling.

# References

Contracts:

- beryl-command-geometry
- expected-action-availability
- scroll-ownership

Widgets:

- command button
- scrollbar

# Anatomy

The code panel consists of a root frame, optional header strip, optional language label, optional command controls, text viewport, code text, and optional vertical resize handle.

Code text is a bounded realization over an owner-supplied range-backed source. Stable byte and logical-line offsets identify text, selections, syntax roles, and copy ranges without requiring the complete source to become one GPUI render tree.

Inline mode omits the root frame and header strip, rendering only the code text inside surrounding content.

# Look

The widget uses monospace typography and preserves source text. Bordered mode reads as a standalone bounded panel. Inline mode reads as code-like text within surrounding prose.

Syntax highlighting is parser-backed and source-preserving: parser output assigns token roles to source ranges and rendering maps those roles through appearance settings.

Languages or labels without a registered parser render as plain text.

# States

Inline, bordered, focused, selected, scrollable, truncated, loading syntax, syntax unavailable, smart-wrap, no-wrap, resized, and disabled command.

# Interaction

The widget's own copy action copies bare plain text.

Callers may define richer copy behavior outside the widget, such as Markdown-preserving transcript copy, without changing the widget's plain-text copy contract.

Optional header controls may include generic actions such as Expand, Collapse, Soft Wrap, Copy, and
`Save…`. Disabled header commands satisfy `expected-action-availability`.

In no-wrap mode, horizontal wheel or direct horizontal scrolling moves the text viewport. In smart-wrap mode, text wraps inside the available inline size.

A nested scrollable code panel does not take vertical pointer-wheel ownership merely because the pointer hovers over it. Clicking the nested code panel selects it for vertical pointer-wheel ownership. While selected, vertical wheel input over that code panel scrolls only the panel and does not co-scroll the outer viewport. Pressing `Escape` does not deselect the nested code panel for pointer-wheel ownership.

Selection and full-source Copy operate on stable source ranges, including ranges outside the current
realization window. Copy reconstructs a contiguous platform representation only after the exact
logical range fits its admitted clipboard limit. Otherwise Copy reports unavailable without loading
the source or changing selection, and owner-supplied `Save…` streams the stable source range to a
selected file through bounded pages. Scrolling, resizing, wrap-mode changes, and syntax-result
publication reconcile the realized range without changing source identity or losing a valid
selection.

# Layout

Smart-wrap prefers breaks on spaces, commas, and semicolons before forcing a split at the last fitting symbol.

No-wrap enables horizontal scrolling instead of soft line breaks.

In bordered mode, the widget may expose a draggable lower edge for vertical resizing within surrounding layout bounds.

Scrollable code panels use the shared scrollbar affordance.

Bordered code panels realize only the visible logical or wrapped text range plus bounded overscan. The viewport consumes range slices and range-indexed syntax roles; it must not eagerly construct GPUI elements, shaped lines, or token children for an unbounded complete source. No-wrap mode uses logical-line ranges. Smart-wrap mode uses a bounded wrap-layout index or equivalent range-backed realization so offscreen text is not shaped merely to build the render tree.

Inline mode is static full render only when the surrounding text contract supplies a documented small source bound. Longer or caller-unbounded code uses bordered mode with the bounded viewport strategy.

Content-free diagnostics expose source byte and logical-line counts, realized source range, realized visual-line count, overscan, wrap mode, viewport size, and range-reconciliation timing. Diagnostics never include source text, copied text, syntax tokens, file paths, or user content.

Spec CSS:

```css
.code-panel {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  min-inline-size: 0;
  background: var(--background);
  color: var(--foreground);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  font-family: var(--font-family);
  font-size: var(--font-size);
}

.code-panel__header {
  display: flex;
  align-items: center;
  block-size: var(--header-height);
  padding-inline: var(--padding-x);
  border-block-end: var(--divider-width) solid var(--divider-color);
}

.code-panel__viewport {
  min-block-size: 0;
  overflow: auto;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
}

.code-panel__text {
  white-space: pre;
  overflow-wrap: normal;
}

.code-panel[data-variant~="smart-wrap"] .code-panel__text {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.code-panel[data-variant~="inline"] {
  display: inline;
  border-width: 0;
  background: transparent;
  padding-inline: var(--inline-padding-x);
}

.code-panel__resize-handle {
  block-size: var(--handle-height);
  cursor: ns-resize;
}
```

# Variants

Inline, bordered, smart-wrap, no-wrap, header-actions, resizable, and readonly.

Default variant: bordered no-wrap.

# UI Roles

```css
.code-panel {
  --background: #f8fafc;
  --foreground: #111827;
  --border-width: 1px;
  --border-color: #cbd5e1;
  --radius: 6px;
  --font-family: monospace;
  --font-size: 13px;
  --inline-padding-x: 2px;
}

.code-panel__header {
  --header-height: 30px;
  --padding-x: 8px;
  --divider-width: 1px;
  --divider-color: #cbd5e1;
}

.code-panel__viewport {
  --padding-x: 10px;
  --padding-y: 8px;
}

.code-panel__resize-handle {
  --handle-height: 5px;
}
```
