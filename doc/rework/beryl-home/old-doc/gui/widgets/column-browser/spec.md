# Name

Canonical name: column browser

Sometimes known as: column selector, Miller columns

# Purpose

Presents traversal through related items as a left-to-right trail of columns.

# References

Contracts:

- scroll-ownership

Widgets:

- scrollbar

# Anatomy

The column browser consists of a horizontal viewport, an ordered column trail, one or more browser columns, optional column headers, and column rows.

A browser column is a vertical list inside the column trail. A root column is the first column. A successor column is opened from the selected row in the previous column.

A column row may be branching or terminal. A branching row opens a successor column. A terminal row does not open a successor column, though it may still select or invoke a caller-defined action.

# Look

The widget reads as one continuous navigation control made of adjacent columns. Columns may use thin dividers or spacing to separate scopes without making each row appear as a separate card.

The selected row in each visible branch remains visually identifiable. The active column may use stronger focus or selection treatment than inactive columns.

# States

Empty, focused, disabled, loading, root-only, multi-column, overflowed, selected row, active column, branching row, terminal row, and truncated row.

# Interaction

Callers own row domain model, row labels, commands, and activation semantics.

Single-click selects a row. Selecting a branching row truncates columns to its right and opens the next column from that row's target.

Selecting a terminal row does not open a next column unless the caller defines it as branching.

Double-click invokes the selected row's caller-defined primary action when one exists.

`Escape` closes the containing feature UI when the owning feature defines that dismissal. `Up` and `Down` move within the active column. `Left` and `Right` move across available columns. `Enter` invokes caller-defined activation.

Only one column-browser instance is interactive at a time when multiple feature overlays or selectors could overlap. Opening one closes conflicting column browsers and their context menus according to feature contracts.

# Layout

The horizontal viewport owns horizontal scrolling when the column trail exceeds available width.

Each browser column owns its own vertical scrolling beneath a fixed one-line header. Browser columns do not share one vertical scroll position.

Column widths are stable while navigating unless the owning feature explicitly defines a responsive width policy. Long labels truncate inside rows instead of resizing sibling columns or causing outer scrolling.

Spec CSS:

```css
.column-browser {
  display: flex;
  align-items: stretch;
  box-sizing: border-box;
  inline-size: 100%;
  block-size: 100%;
  overflow: hidden;
  background: var(--background);
  color: var(--foreground);
}

.column-browser__trail {
  display: flex;
  align-items: stretch;
  min-inline-size: 0;
  overflow: auto hidden;
}

.column-browser__column {
  display: flex;
  flex-direction: column;
  inline-size: var(--column-width);
  min-inline-size: var(--column-min-width);
  max-inline-size: var(--column-max-width);
  border-inline-end: var(--divider-width) solid var(--divider-color);
}

.column-browser__header {
  block-size: var(--header-height);
  padding-inline: var(--padding-x);
  color: var(--header-foreground);
  font-size: var(--header-font-size);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.column-browser__rows {
  min-block-size: 0;
  overflow: hidden auto;
}

.column-browser__row {
  block-size: var(--row-height);
  padding-inline: var(--padding-x);
  color: var(--foreground);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.column-browser__row[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.column-browser__row[data-state~="focused"] {
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}
```

# Variants

Hierarchical column browser and columnar graph browser.

Default variant: hierarchical column browser.

# UI Roles

```css
.column-browser {
  --background: #ffffff;
  --foreground: #1f2937;
}

.column-browser__column {
  --column-width: 260px;
  --column-min-width: 180px;
  --column-max-width: 360px;
  --divider-width: 1px;
  --divider-color: #cbd5e1;
}

.column-browser__header {
  --header-height: 28px;
  --padding-x: 10px;
  --header-foreground: #475569;
  --header-font-size: 12px;
}

.column-browser__row {
  --row-height: 28px;
  --padding-x: 10px;
  --foreground: #1f2937;
}

.column-browser__row[data-state~="selected"] {
  --background: #e0f2fe;
  --foreground: #0f172a;
}

.column-browser__row[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #2563eb;
  --ring-offset: -2px;
}
```
