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

The context menu is a bordered surface with fixed width and fixed height.

Menu items are stacked vertically inside the surface.

If menu items exceed the visible height, the menu scrolls vertically.

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

The context menu has fixed width and fixed height.

Menu items fill the menu width and use a stable row height unless a project-specific item variant defines otherwise.

Vertical scrolling occurs inside the menu surface.

The menu is positioned by the owning invocation context.

# Variants

Action-only, selection-only, mixed action and selection, grouped, disabled-row, and scrollable.

Default variant: mixed action and selection.

# UI Roles

## Root

- `width`: `240px`
- `height`: `320px`
- `padding-y`: `4px`
- `radius`: `6px`
- `border-width`: `1px`
- `background`: `#ffffff`
- `foreground`: `#111827`
- `border-color`: `#cbd5e1`
- `shadow`: `0 12px 28px rgba(15, 23, 42, 0.18)`
- `font-size`: `13px`

## Parts

### `item`

- `height`: `28px`
- `padding-x`: `10px`
- `gap`: `8px`
- `radius`: `4px`

#### States

##### `hover`

- `background`: `#eef2f7`
- `foreground`: `#0f172a`

##### `pressed`

- `background`: `#e2e8f0`
- `foreground`: `#0f172a`

##### `focused`

- `background`: `#dbeafe`
- `foreground`: `#0f172a`

##### `selected`

- `background`: `#eff6ff`
- `foreground`: `#1d4ed8`

##### `disabled`

- `foreground`: `#94a3b8`
- `opacity`: `1`

### `header`

- `height`: `24px`
- `padding-x`: `10px`
- `foreground`: `#64748b`
- `font-size`: `12px`
- `font-weight`: `500`

### `separator`

- `height`: `1px`
- `background`: `#e2e8f0`

### `checkmark`

- `size`: `16px`
- `foreground`: `#2563eb`
