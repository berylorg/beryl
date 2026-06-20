# Name

Canonical name: anchored context menu

Sometimes known as: flyout context menu, context flyout, anchored menu

# Purpose

Presents a context menu anchored to the control or segment that opened it.

# Anatomy

The anchored context menu consists of an anchor, a bordered menu surface, a vertical scroll container, and menu items.

Menu items are rendered as full-width rows.

The menu item types match the context menu spec: action items, selection items, and text header items.

# Look

The anchored context menu has the same visual structure as a context menu.

The menu appears near its anchor and reads as visually connected to the control, segment, or region that opened it.

The menu surface has a border, fixed width, fixed height, and vertically scrollable contents when needed.

# States

Closed, open, anchored, focused, scrollable, item-normal, item-hover, item-pressed, item-focused, item-selected, and item-disabled.

# Interaction

Opening the anchored context menu associates it with its anchor.

Action item activation invokes the item's command and closes the menu.

Selection item activation marks that item visually selected and reports that item as the selected element.

Text header items are not interactive.

Arrow Up and Arrow Down move focus among interactive menu items.

Home and End move focus to the first and last interactive menu items.

Enter and Space activate the focused interactive menu item.

Escape, outside click, anchor reactivation, or an equivalent dismissal action closes the flyout unless the owning environment defines a stricter dismissal rule.

The anchored context menu does not define the behavior of the control that opened it beyond anchoring and dismissal.

# Layout

The menu is positioned relative to its anchor.

The preferred placement is near the anchor without obscuring the anchor more than necessary.

If the preferred placement would overflow the viewport or containing surface, the menu may flip, shift, or clamp while remaining associated with the anchor.

The menu keeps fixed width and fixed height. Vertical scrolling occurs inside the menu surface.

# Variants

Above-anchor, below-anchor, leading-edge aligned, trailing-edge aligned, flipped, shifted, and clamped.

Default variant: below-anchor, leading-edge aligned.

# UI Roles

## Root

- `anchor-gap`: `4px`
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
