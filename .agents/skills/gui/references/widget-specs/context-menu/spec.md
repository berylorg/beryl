# Name

Canonical name: context menu

Sometimes known as: shortcut menu, contextual menu, right-click menu

# Purpose

Presents contextual actions or selections related to the current object, region, or invocation context.

# References

Contracts:

- disabled-command-tooltip

# Anatomy

The context menu consists of a bordered menu panel, a vertical scroll container, and menu rows. The virtualized-collection variant additionally owns a fixed-row realization window and total scroll extent for caller-unbounded selectable options.

Menu rows are rendered full-width.

Menu rows may be command rows, selection rows, submenu rows, toggle rows, or text header rows.

A command row invokes a command.

A selection row represents a selectable value.

A text header row labels a group of rows and is not interactive.

# Look

The context menu is a bordered vertical menu panel.

Command rows and selection rows use hover, pressed, focused, and selected visuals as appropriate.

Text header rows use a quieter text treatment to separate or label row groups.

# States

Closed, open, focused, scrollable, static-full-render, virtualized-collection, row-normal, row-hover, row-pressed, row-focused, row-selected, and row-disabled.

# Interaction

Opening the context menu presents the menu rows for the current context.

Clicking or tapping an enabled command row invokes that row's command and closes the context menu.

Clicking or tapping an enabled selection row marks that row visually selected and reports that row as the selected element.

Selection rows do not necessarily close the context menu unless the owning feature defines that behavior.

Text header rows are not interactive.

Arrow Up and Arrow Down move focus among interactive rows.

Home and End move focus to the first and last interactive rows.

Enter and Space activate the focused interactive row.

Row activation and logical keyboard focus use caller-supplied stable row ids, never realized row indices. In the virtualized-collection variant, keyboard movement reveals and realizes the logically focused row before activation. Same-collection refreshes preserve logical focus, selection, and scroll position when their stable ids remain present.

Disabled command rows must satisfy `disabled-command-tooltip`.

The menu can scroll vertically when its rows exceed the visible height.

Escape, outside click, or an equivalent dismissal action closes the context menu unless the owning environment defines a stricter dismissal rule.

# Layout

The menu is positioned by the owning invocation context.

The default context-menu strategy is static full rendering: every row in one open menu is mounted. This strategy is valid only for a caller-owned command or selection set whose surrounding model contract documents a small maximum row count. The caller must enforce that maximum with a test or source assertion.

The virtualized-collection variant is the menu's picker-style mode for a caller-unbounded set of selectable options. It stores total row count separately, uses one fixed row height, computes a visible logical range from scroll offset and viewport height, realizes only that range plus fixed bounded overscan, and preserves total extent through spacers or an equivalent GPUI list primitive. Row presentation data is range-backed and precomputed outside the row render hot path.

Virtualized collection updates preserve stable row identity, selected-row reveal, logical keyboard focus, scroll position, and row command dispatch. A tooltip or submenu anchored to a row that leaves the realized range closes intentionally. Resize, theme, font-metric, and collection-revision changes reconcile the visible range without mounting the complete collection.

Caller-unbounded command sets or collections that need search, filtering, variable-height rows, retained inputs, or richer picker behavior must use a separate purpose-built virtualized picker or selector. A context menu may contain a bounded command that opens that surface.

Internal scrolling in the default variant only keeps a caller-bounded menu within the available viewport; it does not relax the static row-count bound.

Content-free diagnostics for the virtualized-collection variant expose total row count, realized row count, visible logical range, overscan count, fixed-row strategy, scroll offset, logical-focus presence, selection presence, and range-reconciliation timing. Diagnostics never include row labels, values, commands, or raw stable ids.

The CSS block defines content-derived menu sizing, row sizing, clamping, and internal scrolling.

Spec CSS:

```css
.context-menu {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  inline-size: clamp(var(--min-width), max-content, min(var(--max-width), available-inline-size));
  max-block-size: min(var(--max-height), available-block-size);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  box-shadow: var(--shadow);
  font-size: var(--font-size);
  overflow-y: auto;
}

.context-menu__row {
  display: flex;
  align-items: center;
  box-sizing: border-box;
  block-size: calc(measure("M", var(--font-size), var(--font-weight)) + 2 * var(--padding-y));
  inline-size: 100%;
  padding-inline: var(--padding-x);
  gap: var(--gap);
  border-radius: var(--radius);
  font-weight: var(--font-weight);
  white-space: nowrap;
}

.context-menu__row[data-state~="hover"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__row[data-state~="pressed"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__row[data-state~="focused"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__row[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__row[data-state~="disabled"] {
  color: var(--foreground);
  opacity: var(--opacity);
}

.context-menu__header {
  box-sizing: border-box;
  block-size: calc(measure("M", var(--font-size), var(--font-weight)) + 2 * var(--padding-y));
  padding-inline: var(--padding-x);
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  white-space: nowrap;
}

.context-menu__separator {
  block-size: var(--height);
  background: var(--background);
}

.context-menu__checkmark {
  inline-size: var(--size);
  block-size: var(--size);
  color: var(--foreground);
}
```

# Variants

Command-only, selection-only, mixed command and selection, grouped, disabled-row, submenu-row, toggle-row, scrollable, static-full-render, and virtualized-collection.

Default variant: mixed command and selection with static full rendering.

# UI Roles

```css
.context-menu {
  --min-width: 160px;
  --max-width: 480px;
  --max-height: 320px;
  --padding-y: 4px;
  --radius: 6px;
  --border-width: 1px;
  --background: #ffffff;
  --foreground: #111827;
  --border-color: #cbd5e1;
  --shadow: 0 12px 28px rgba(15, 23, 42, 0.18);
  --font-size: 13px;
}

.context-menu__row {
  --padding-x: 10px;
  --padding-y: 6px;
  --gap: 8px;
  --radius: 4px;
  --font-weight: 400;
}

.context-menu__row[data-state~="hover"] {
  --background: #eef2f7;
  --foreground: #0f172a;
}

.context-menu__row[data-state~="pressed"] {
  --background: #e2e8f0;
  --foreground: #0f172a;
}

.context-menu__row[data-state~="focused"] {
  --background: #dbeafe;
  --foreground: #0f172a;
}

.context-menu__row[data-state~="selected"] {
  --background: #eff6ff;
  --foreground: #1d4ed8;
}

.context-menu__row[data-state~="disabled"] {
  --foreground: #94a3b8;
  --opacity: 1;
}

.context-menu__header {
  --padding-x: 10px;
  --padding-y: 5px;
  --foreground: #64748b;
  --font-size: 12px;
  --font-weight: 500;
}

.context-menu__separator {
  --height: 1px;
  --background: #e2e8f0;
}

.context-menu__checkmark {
  --size: 16px;
  --foreground: #2563eb;
}
```
