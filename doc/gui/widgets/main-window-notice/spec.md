# Name

Canonical name: main-window notice

Sometimes known as: conversation-window notice, overlay notice

# Purpose

Presents one bounded warning, error, or informational message, with optional owner commands, over a main conversation window without replacing or reflowing the shell.

# References

Contracts:

- disabled-command-tooltip
- scroll-ownership

Widgets:

- command button
- scrollbar

# Anatomy

The main-window notice contains an overlay-anchored frame, variant marker, fixed header,
owner-supplied bounded title, optional close command, optional bounded selectable detail viewport,
optional owner-supplied command region, and external vertical scrollbar.

The widget presents exactly one owner-supplied notice record at a time. The owning feature supplies
stable notice identity, bounded title and detail projections, warning/error/info classification,
dismissal mode and effect, optional commands with stable command identities, an overlay-local origin
derived from the block-end edge of current main-window chrome, logical safe insets, the available
overlay allocation, and a safe focus target. The command region accepts at most three commands and
renders that statically bounded set in full. Queue policy, deduplication, same-condition updates,
and replacement order remain outside the widget. The widget owns visible notice anatomy, detail
selection, close-control placement, command-region placement, bounded layout, and variant treatment.

Owner commands use the referenced `command button` contract. The widget does not prescribe command labels, command effects, pending policy, retry semantics, a global dismiss-all command, or any other feature-specific recovery policy.

# Look

The notice reads as a compact overlay panel above the conversation shell. Variant styling changes marker, border, background, and foreground without changing shared anatomy or control placement.

The title remains visually prominent. Detail text is selectable and wraps inside its bounded viewport. Any close command and owner-supplied command region remain reachable regardless of detail length.

The frame keeps one stable inline size across notices. Replacing a notice never moves or resizes the underlying toolbar, lineage, transcript, activity panel, composer, or status line.

# States

The widget supports entering, visible, leaving, warning, error, info, dismissible, persistent, detail absent, detail present, detail overflow, detail scrolled, detail selection active, commands absent, commands present, close focused, owner command focused, owner command loading, replacing, and inert states.

Replacement publishes the new stable notice identity, content, and variant as one visible update. It never briefly stacks the outgoing and incoming records.

# Interaction

In the dismissible variant, the close command has the accessible name `Dismiss notice` and dismisses only the exact stable notice identity supplied by the owner. Enter and Space activate it when focused. Duplicate activation while dismissal is pending is rejected.

The persistent variant renders no close command and does not dismiss through Escape, outside click, or notice activation. Its owner removes or replaces the record when the feature-owned condition ends.

Enabled owner commands follow the `command button` interaction contract and report their stable command identity to the owner. Command activation does not implicitly dismiss the notice. Disabled owner commands retain the referenced `disabled-command-tooltip` obligation, and loading or duplicate-activation policy remains owner supplied.

Detail text supports ordinary pointer and keyboard selection and copy. It is readonly and contains no transcript or feature context menu. The detail viewport owns vertical scrolling while it has overflow; boundary propagation follows `scroll-ownership`.

When replacement changes the stable notice identity, old detail selection and scroll position are cleared. If the previously focused notice control is absent in the replacement, focus moves to the first enabled owner command, then the close command, then the owner-supplied safe target. If owner removal supplies no replacement record, focus returns to that safe target in the unchanged main window.

When content for the same stable notice identity receives a newer revision, the widget preserves focus on a retained close command or stable owner command and preserves detail scroll only while the prior top-visible text geometry remains valid. It clears a selection whose source revision changed.

While inert, the notice frame and all descendants are excluded from focus routing and reject
pointer, wheel, touchpad, scrollbar, keyboard selection or copy, close-command, owner-command, and
programmatic acceptance input. The widget emits no dismissal, owner-command, selection, copy, or
scroll callback. If it becomes inert while a notice control holds focus, focus moves to the
owner-supplied safe target.

The detail viewport contains one owner-supplied bounded text record, not a repeated list. Queue size does not affect the widget's render tree because only the current record is mounted.

Content-free diagnostics expose widget instance id, an opaque nonreversible notice diagnostic key, content revision, severity variant, dismissal variant, visibility state, detail presence, command count, allocated size, overflow presence, scroll offset, selection presence, notice-control focus kind, and replacement count. Diagnostics never include title, detail, command labels, errors, paths, raw notice or command ids, queue content, or copied text.

# Layout

The owner supplies the overlay-local block-start origin immediately below the current toolbar and
any visible lineage strip, together with logical safe insets and the available overlay allocation.
The frame anchors at that origin near the trailing edge, applies the owner-supplied insets, and
clamps its fixed inline size and content-derived capped block size to that allocation.

The fixed header places the variant marker and title leading and the optional close command trailing. The detail viewport fills the remaining bounded allocation and never displaces header or command controls. Its scrollbar overlays the trailing edge.

The optional command region follows the detail viewport, aligns its command buttons to the trailing edge, and wraps within the frame rather than increasing the frame's inline size. Header and command controls remain fixed while overflowing detail scrolls.

Entering, leaving, and replacement treatments use overlay-only paint changes. They do not contribute layout space to the main conversation window.

`--owner-origin-block-start`, `--owner-inset-block-start`, and
`--owner-inset-inline-end` are dynamic overlay-local values supplied by the owning main-window
chrome after accounting for the current toolbar, lineage strip, and safe area.

Spec CSS:

```css
.main-window-notice {
  position: absolute;
  inset-block-start: calc(var(--owner-origin-block-start) + var(--owner-inset-block-start));
  inset-inline-end: var(--owner-inset-inline-end);
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  inline-size: min(var(--width), available-inline-size);
  max-block-size: min(var(--max-height), available-block-size);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  box-shadow: var(--shadow);
}

.main-window-notice__header {
  display: flex;
  flex: none;
  align-items: center;
  min-inline-size: 0;
  block-size: var(--height);
  padding-inline: var(--padding-x);
  gap: var(--gap);
}

.main-window-notice__title {
  flex: 1 1 auto;
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
}

.main-window-notice__variant-marker {
  flex: none;
  inline-size: var(--size);
  block-size: var(--size);
  border-radius: var(--radius);
  background: var(--background);
}

.main-window-notice__close {
  flex: none;
}

.main-window-notice__detail-viewport {
  position: relative;
  min-block-size: 0;
  max-block-size: var(--max-height);
  padding-inline: var(--padding-x);
  padding-block-end: var(--padding-y);
  overflow: hidden;
  color: var(--foreground);
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.main-window-notice__command-region {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  padding-inline: var(--padding-x);
  padding-block-end: var(--padding-y);
  gap: var(--gap);
}

.main-window-notice[data-state~="entering"],
.main-window-notice[data-state~="leaving"] {
  opacity: var(--opacity);
}

.main-window-notice[data-state~="inert"] {
  opacity: var(--opacity);
}
```

# Variants

Severity variants are warning, error, and info.

The active severity token is applied to the root frame and projected to every severity-sensitive
anatomy part: variant marker, title, detail viewport, and close command. Each such part resolves the
matching warning, error, or info UI-role selector from the same visible notice revision. Owner
`command button` controls retain their separately configured variant.

Dismissal variants are dismissible and persistent. Dismissible renders the close command. Persistent omits the close command and remains until its owner removes or replaces the notice.

Default variant: info dismissible.

# UI Roles

```css
.main-window-notice {
  --owner-origin-block-start: 0px;
  --owner-inset-block-start: 12px;
  --owner-inset-inline-end: 12px;
  --width: 420px;
  --max-height: 280px;
  --border-width: 1px;
  --border-color: #334155;
  --radius: 8px;
  --background: #111827;
  --foreground: #e5e7eb;
  --shadow: 0 12px 28px rgba(0, 0, 0, 0.42);
}

.main-window-notice__header {
  --height: 40px;
  --padding-x: 12px;
  --gap: 8px;
}

.main-window-notice__title {
  --foreground: #f8fafc;
  --font-size: 13px;
  --font-weight: 650;
}

.main-window-notice__variant-marker {
  --size: 8px;
  --radius: 999px;
  --background: #38bdf8;
}

.main-window-notice__variant-marker[data-variant~="warning"] {
  --background: #f59e0b;
}

.main-window-notice__variant-marker[data-variant~="error"] {
  --background: #ef4444;
}

.main-window-notice__variant-marker[data-variant~="info"] {
  --background: #38bdf8;
}

.main-window-notice__detail-viewport {
  --max-height: 220px;
  --padding-x: 12px;
  --padding-y: 10px;
  --foreground: #cbd5e1;
}

.main-window-notice__command-region {
  --padding-x: 12px;
  --padding-y: 10px;
  --gap: 8px;
}

.main-window-notice__close {
  --background: transparent;
  --foreground: #cbd5e1;
  --border-color: transparent;
}

.main-window-notice[data-variant~="warning"] {
  --background: #2b2110;
  --foreground: #fde68a;
  --border-color: #a16207;
}

.main-window-notice__title[data-variant~="warning"] {
  --foreground: #fef3c7;
}

.main-window-notice__detail-viewport[data-variant~="warning"],
.main-window-notice__close[data-variant~="warning"] {
  --foreground: #fde68a;
}

.main-window-notice[data-variant~="error"] {
  --background: #2b1518;
  --foreground: #fecaca;
  --border-color: #b91c1c;
}

.main-window-notice__title[data-variant~="error"] {
  --foreground: #fee2e2;
}

.main-window-notice__detail-viewport[data-variant~="error"],
.main-window-notice__close[data-variant~="error"] {
  --foreground: #fecaca;
}

.main-window-notice[data-variant~="info"] {
  --background: #10243a;
  --foreground: #bae6fd;
  --border-color: #0369a1;
}

.main-window-notice__title[data-variant~="info"] {
  --foreground: #e0f2fe;
}

.main-window-notice__detail-viewport[data-variant~="info"],
.main-window-notice__close[data-variant~="info"] {
  --foreground: #bae6fd;
}

.main-window-notice[data-state~="entering"],
.main-window-notice[data-state~="leaving"] {
  --opacity: 0;
}

.main-window-notice[data-state~="inert"] {
  --opacity: 0.62;
}
```
