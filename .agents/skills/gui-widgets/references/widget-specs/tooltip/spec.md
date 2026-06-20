# Name

Canonical name: tooltip

Sometimes known as: hover tooltip, help tooltip

# Purpose

Shows brief explanatory text for an element without accepting interaction.

# References

N/A

# Anatomy

The tooltip consists of a transient text surface and optional pointer notch.

The owning element provides the tooltip text and anchor.

# Look

The tooltip is a compact transient surface with readable helper text.

The tooltip should visually sit above ordinary content without reading as a menu, dialog, or notification.

# States

Closed, opening, visible, closing, and clamped.

# Interaction

The tooltip is opened by the owning element's hover, focus, or equivalent inspect gesture.

The tooltip does not receive focus, accept selection, expose commands, or contain interactive controls.

The tooltip closes when the owning hover, focus, or inspect gesture ends, or when the owning surface dismisses transient affordances.

# Layout

The tooltip is positioned near its anchor without obscuring the anchor more than necessary.

If the preferred placement would overflow the viewport or containing surface, the tooltip may flip, shift, or clamp while remaining associated with the anchor.

The CSS block defines compact tooltip surface sizing, text wrapping, and optional notch geometry.

Spec CSS:

```css
.tooltip {
  position: absolute;
  box-sizing: border-box;
  inline-size: max-content;
  max-inline-size: min(var(--max-width), available-inline-size);
  max-block-size: min(var(--max-height), available-block-size);
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  box-shadow: var(--shadow);
  font-size: var(--font-size);
  line-height: var(--line-height);
  overflow-wrap: break-word;
  pointer-events: none;
}

.tooltip[data-state~="opening"] {
  opacity: var(--opacity);
}

.tooltip[data-state~="visible"] {
  opacity: var(--opacity);
}

.tooltip[data-state~="closing"] {
  opacity: var(--opacity);
}

.tooltip__notch {
  inline-size: var(--size);
  block-size: var(--size);
  background: var(--background);
  border: var(--border-width) solid var(--border-color);
}
```

# Variants

Plain, warning, error, above-anchor, below-anchor, leading-edge aligned, trailing-edge aligned, flipped, shifted, and clamped.

Default variant: plain.

# UI Roles

```css
.tooltip {
  --max-width: 320px;
  --max-height: 160px;
  --padding-x: 8px;
  --padding-y: 6px;
  --radius: 5px;
  --border-width: 1px;
  --background: #111827;
  --foreground: #ffffff;
  --border-color: #0f172a;
  --shadow: 0 10px 24px rgba(15, 23, 42, 0.22);
  --font-size: 12px;
  --line-height: 16px;
}

.tooltip[data-state~="opening"] {
  --opacity: 0;
}

.tooltip[data-state~="visible"] {
  --opacity: 1;
}

.tooltip[data-state~="closing"] {
  --opacity: 0;
}

.tooltip__notch {
  --size: 8px;
  --background: #111827;
  --border-width: 1px;
  --border-color: #0f172a;
}
```
