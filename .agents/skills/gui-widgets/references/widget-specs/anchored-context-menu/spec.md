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

The CSS block defines content-derived menu sizing, row sizing, clamping, and internal scrolling.

Spec CSS:

```css
.anchored-context-menu {
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  inline-size: clamp(var(--min-width), max-content, min(var(--max-width), available-inline-size));
  max-block-size: min(var(--max-height), available-block-size);
  margin-block-start: var(--anchor-gap);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  box-shadow: var(--shadow);
  font-size: var(--font-size);
  overflow-y: auto;
}

.anchored-context-menu__item {
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

.anchored-context-menu__item[data-state~="hover"] {
  background: var(--background);
  color: var(--foreground);
}

.anchored-context-menu__item[data-state~="pressed"] {
  background: var(--background);
  color: var(--foreground);
}

.anchored-context-menu__item[data-state~="focused"] {
  background: var(--background);
  color: var(--foreground);
}

.anchored-context-menu__item[data-state~="selected"] {
  background: var(--background);
  color: var(--foreground);
}

.anchored-context-menu__item[data-state~="disabled"] {
  color: var(--foreground);
  opacity: var(--opacity);
}

.anchored-context-menu__header {
  box-sizing: border-box;
  block-size: calc(measure("M", var(--font-size), var(--font-weight)) + 2 * var(--padding-y));
  padding-inline: var(--padding-x);
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  white-space: nowrap;
}

.anchored-context-menu__separator {
  block-size: var(--height);
  background: var(--background);
}

.anchored-context-menu__checkmark {
  inline-size: var(--size);
  block-size: var(--size);
  color: var(--foreground);
}
```

# Variants

Above-anchor, below-anchor, leading-edge aligned, trailing-edge aligned, flipped, shifted, and clamped.

Default variant: below-anchor, leading-edge aligned.

# UI Roles

```css
.anchored-context-menu {
  --anchor-gap: 4px;
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

.anchored-context-menu__item {
  --padding-x: 10px;
  --padding-y: 6px;
  --gap: 8px;
  --radius: 4px;
}

.anchored-context-menu__item[data-state~="hover"] {
  --background: #eef2f7;
  --foreground: #0f172a;
}

.anchored-context-menu__item[data-state~="pressed"] {
  --background: #e2e8f0;
  --foreground: #0f172a;
}

.anchored-context-menu__item[data-state~="focused"] {
  --background: #dbeafe;
  --foreground: #0f172a;
}

.anchored-context-menu__item[data-state~="selected"] {
  --background: #eff6ff;
  --foreground: #1d4ed8;
}

.anchored-context-menu__item[data-state~="disabled"] {
  --foreground: #94a3b8;
  --opacity: 1;
}

.anchored-context-menu__header {
  --padding-x: 10px;
  --padding-y: 5px;
  --foreground: #64748b;
  --font-size: 12px;
  --font-weight: 500;
}

.anchored-context-menu__separator {
  --height: 1px;
  --background: #e2e8f0;
}

.anchored-context-menu__checkmark {
  --size: 16px;
  --foreground: #2563eb;
}
```
