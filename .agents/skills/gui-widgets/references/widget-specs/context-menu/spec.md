# Name

Canonical name: context menu

Sometimes known as: shortcut menu, contextual menu, right-click menu

# Purpose

Presents contextual actions or selections related to the current object, region, or invocation context.

# Anatomy

The context menu consists of a bordered menu surface, a vertical scroll container, and menu items.

Menu items are rendered as full-width rows.

Menu items may be action items, selection items, or text header items.

An action item invokes a command.

A selection item represents a selectable value.

A text header item labels a group of menu items and is not interactive.

# Look

The context menu is a bordered vertical menu surface.

Action items and selection items use hover, pressed, focused, and selected visuals as appropriate.

Text header items use a quieter text treatment to separate or label groups of action and selection items.

# States

Closed, open, focused, scrollable, item-normal, item-hover, item-pressed, item-focused, item-selected, and item-disabled.

# Interaction

Opening the context menu presents the menu items for the current context.

Clicking or tapping an enabled action item invokes that item's command and closes the context menu.

Clicking or tapping an enabled selection item marks that item visually selected and reports that item as the selected element.

Selection items do not necessarily close the context menu unless the owning feature defines that behavior.

Text header items are not interactive.

Arrow Up and Arrow Down move focus among interactive menu items.

Home and End move focus to the first and last interactive menu items.

Enter and Space activate the focused interactive menu item.

The menu can scroll vertically when its items exceed the visible height.

Escape, outside click, or an equivalent dismissal action closes the context menu unless the owning environment defines a stricter dismissal rule.

# Layout

The menu is positioned by the owning invocation context.

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

.context-menu__item {
  display: flex;
  align-items: center;
  box-sizing: border-box;
  block-size: calc(measure("M", var(--font-size), 400) + 2 * var(--padding-y));
  inline-size: 100%;
  padding-inline: var(--padding-x);
  gap: var(--gap);
  border-radius: var(--radius);
  white-space: nowrap;
}

.context-menu__item[data-state~="hover"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__item[data-state~="pressed"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__item[data-state~="focused"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__item[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.context-menu__item[data-state~="disabled"] {
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

Action-only, selection-only, mixed action and selection, grouped, disabled-row, and scrollable.

Default variant: mixed action and selection.

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

.context-menu__item {
  --padding-x: 10px;
  --padding-y: 6px;
  --gap: 8px;
  --radius: 4px;
}

.context-menu__item[data-state~="hover"] {
  --background: #eef2f7;
  --foreground: #0f172a;
}

.context-menu__item[data-state~="pressed"] {
  --background: #e2e8f0;
  --foreground: #0f172a;
}

.context-menu__item[data-state~="focused"] {
  --background: #dbeafe;
  --foreground: #0f172a;
}

.context-menu__item[data-state~="selected"] {
  --background: #eff6ff;
  --foreground: #1d4ed8;
}

.context-menu__item[data-state~="disabled"] {
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
