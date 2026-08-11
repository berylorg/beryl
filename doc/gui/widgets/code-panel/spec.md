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

The code panel consists of a root frame, optional header strip, optional language label, optional
command controls, text viewport, code text, optional feedback part, and optional vertical resize
handle.

Code text is a bounded realization over an owner-supplied range-backed source. Stable byte and logical-line offsets identify text, selections, syntax roles, and copy ranges without requiring the complete source to become one GPUI render tree. The `smart-wrap` variant is projected onto the code-text part so styling requires no descendant selector.

Each bordered code-panel instance has one owner-supplied stable panel identity. The root, text
viewport, scrollbar event target, selected-for-wheel state, and retained viewport anchor keep that
identity across same-source revisions, syntax-result publication, wrap-mode changes, resize,
realization-window changes, and owner-supplied feedback changes. Only owner replacement of the
logical panel changes panel identity.

The viewport retains a top-visible anchor as the current source identity, an owner-resolvable stable
logical-line identity, an intra-line byte offset, and its visual block offset from the viewport
start. Same-source revision results supply a bounded mapping for that stable line and offset when
edits can move or remove it; the code panel does not infer a revision mapping by scanning changed
text.

Bordered mode may include one optional feedback part after the text viewport and before the resize
handle. It presents one owner-supplied bounded status or validation message. The owner supplies the
message, complete accessibility text, semantic meaning, and invalidity; the code panel owns only
placement and presentation and never performs validation. The feedback part is passive and is not
a focus stop, command, or scroll surface.

Inline mode omits the root frame, header strip, feedback part, and resize handle, rendering only the
code text inside surrounding content.

# Look

The widget uses monospace typography and preserves source text. Bordered mode reads as a standalone bounded panel. Inline mode reads as code-like text within surrounding prose.

Syntax highlighting is parser-backed and source-preserving: parser output assigns token roles to source ranges and rendering maps those roles through appearance settings.

Languages or labels without a registered parser render as plain text.

When a bordered scrollable panel is selected for vertical pointer-wheel ownership, the complete
root shows a persistent selected outline. The treatment remains visible when focus moves and does
not change panel geometry.

Feedback reads as a compact status region subordinate to code content. Invalid feedback remains
distinct from neutral status without changing its bounded allocation.

# States

Inline, bordered, focused, selected, scrollable, truncated, loading syntax, syntax unavailable,
smart-wrap, no-wrap, resized, feedback present, feedback invalid, and disabled command.

The root selected state means selected for vertical-wheel ownership. It is distinct from text
selection and keyboard focus.

# Interaction

The widget's own copy action copies bare plain text.

Callers may define richer copy behavior outside the widget, such as Markdown-preserving transcript copy, without changing the widget's plain-text copy contract.

Optional header controls may include generic actions such as Expand, Collapse, Soft Wrap, Copy, and
`Save…`. Disabled header commands satisfy `expected-action-availability`.

Owner feedback updates in place without changing stable panel or source identity, text selection,
focus, or selected-for-wheel ownership. Any viewport-allocation change uses the anchor
reconciliation below.

In no-wrap mode, horizontal wheel or direct horizontal scrolling moves the text viewport. In smart-wrap mode, text wraps inside the available inline size.

A nested scrollable code panel does not acquire vertical pointer-wheel ownership from hover. A
primary pointer press in its text viewport or direct interaction with its scrollbar requests
acquisition for the panel's stable identity from the containing scroll router; header-command and
resize-handle activation do not acquire it. The router exposes at most one selected nested vertical
wheel owner.

A later acquisition by another eligible nested scroll surface transfers ownership atomically: the
prior panel clears selected before the new panel exposes it. A primary pointer press routed to the
outer viewport outside the selected panel releases ownership to that outer viewport. Unmounting or
replacing the selected stable panel identity, loss of vertical overflow, or an explicit owner clear
also releases it. Hover changes, focus movement, and Escape do not release it; if overflow later
returns, selection is not reacquired automatically.

While selected, vertical wheel or touchpad input originating over the panel routes only to that
panel. Reaching a panel scroll boundary neither co-scrolls the outer viewport nor releases or
transfers ownership. When the panel is not selected, vertical wheel or touchpad input over it remains
routed to the outer viewport. One gesture has exactly one vertical owner.

Selection and full-source Copy operate on stable source ranges, including ranges outside the current
realization window. Copy reconstructs a contiguous platform representation only after the exact
logical range fits its admitted clipboard limit. Otherwise Copy reports unavailable without loading
the source or changing selection, and owner-supplied `Save…` streams the stable source range to a
selected file through bounded pages. Scrolling, resizing, wrap-mode changes, and syntax-result
publication reconcile the realized range without changing source identity or losing a valid
selection.

Scrolling updates the retained top-visible anchor. Before a wrap-mode change, viewport-width or
block-size change, resize, same-source revision adoption, syntax-layout publication, or
owner-supplied feedback change alters text geometry, the widget captures the top-visible stable
logical-line identity, its intra-line byte offset, and its visual block offset. After bounded
relayout it resolves that position in the new revision and places it at the same visual offset,
clamped only by the new scroll extent; no-wrap mode also preserves the prior inline scroll offset
subject to clamping. These changes preserve stable panel identity and a still-valid text selection.

If a same-source revision deletes the anchor, the owner-supplied bounded mapping resolves it to the
nearest surviving position with a leading-side bias, and the widget preserves the prior visual
offset where the new extent permits. An empty result clamps to the viewport origin. A changed source
identity uses the owner-supplied initial viewport position instead of reusing the old source anchor.

# Layout

Smart-wrap prefers breaks on spaces, commas, and semicolons before forcing a split at the last fitting symbol.

No-wrap enables horizontal scrolling instead of soft line breaks.

In bordered mode, the widget may expose a draggable lower edge for vertical resizing within surrounding layout bounds.

When present, feedback spans the bordered panel below the viewport and above the resize handle. It
consumes only its bounded allocation, clips overflow while retaining complete owner-supplied
accessibility text, and does not become another scroll container.

Scrollable code panels use the shared scrollbar affordance.

Bordered code panels realize only the visible logical or wrapped text range plus bounded overscan. The viewport consumes range slices and range-indexed syntax roles; it must not eagerly construct GPUI elements, shaped lines, or token children for an unbounded complete source. No-wrap mode uses logical-line ranges. Smart-wrap mode uses a bounded wrap-layout index or equivalent range-backed realization so offscreen text is not shaped merely to build the render tree.

The top-visible anchor is source-based rather than realization-index based. Realization-window
replacement, overscan changes, and visual-line reindexing therefore cannot substitute the first
newly realized row for the retained source position. Smart-wrap maps the retained stable line and
intra-line byte offset into the new wrapped visual line before restoring its visual block offset.

Inline mode is static full render only when the surrounding text contract supplies a documented small source bound. Longer or caller-unbounded code uses bordered mode with the bounded viewport strategy.

Content-free diagnostics expose a nonreversible stable-panel diagnostic key, source-identity
continuity, source byte and logical-line counts, realized source range, realized visual-line count,
overscan, wrap mode, viewport size, selected-for-wheel presence, retained-anchor presence,
anchor-remap result kind, anchor visual offset, and range-reconciliation timing. Diagnostics never
include raw panel or source identities, source anchor positions, source text, copied text, syntax
tokens, file paths, or user content.

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

.code-panel[data-state~="selected"] {
  border-color: var(--border-color);
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.code-panel__header {
  display: flex;
  align-items: center;
  block-size: var(--height);
  padding-inline: var(--padding-x);
  border-block-end: var(--separator-width) solid var(--separator-color);
}

.code-panel__viewport {
  min-block-size: 0;
  overflow: auto;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
}

.code-panel__feedback {
  flex: none;
  box-sizing: border-box;
  max-block-size: var(--max-height);
  overflow: hidden;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border-block-start: var(--separator-width) solid var(--separator-color);
  background: var(--background);
  color: var(--foreground);
  font-size: var(--font-size);
  line-height: var(--line-height);
  overflow-wrap: anywhere;
}

.code-panel__feedback[data-state~="invalid"] {
  background: var(--background);
  color: var(--foreground);
}

.code-panel__text {
  white-space: pre;
  overflow-wrap: normal;
}

.code-panel__text[data-variant~="smart-wrap"] {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.code-panel[data-variant~="inline"] {
  display: inline;
  border-width: 0;
  background: var(--background);
  padding-inline: var(--inline-padding-x);
}

.code-panel__resize-handle {
  block-size: var(--height);
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

.code-panel[data-state~="selected"] {
  --border-color: #38bdf8;
  --ring-width: 1px;
  --ring-color: #38bdf8;
  --ring-offset: -1px;
}

.code-panel__header {
  --height: 30px;
  --padding-x: 8px;
  --separator-width: 1px;
  --separator-color: #cbd5e1;
}

.code-panel__viewport {
  --padding-x: 10px;
  --padding-y: 8px;
}

.code-panel__feedback {
  --max-height: 48px;
  --padding-x: 8px;
  --padding-y: 6px;
  --separator-width: 1px;
  --separator-color: #cbd5e1;
  --background: #f1f5f9;
  --foreground: #475569;
  --font-size: 12px;
  --line-height: 16px;
}

.code-panel__feedback[data-state~="invalid"] {
  --background: #fef2f2;
  --foreground: #b91c1c;
}

.code-panel__resize-handle {
  --height: 5px;
}

.code-panel[data-variant~="inline"] {
  --background: transparent;
}
```
