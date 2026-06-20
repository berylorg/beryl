# Name

Canonical name: scrollbar

Sometimes known as: scroll bar

# Purpose

Shows scroll position for content inside a viewport and provides direct manipulation for changing that scroll position.

# References

N/A

# Anatomy

The scrollbar is associated with one scroll container, one viewport, and one scroll axis.

The scrollbar contains a thumb. The thumb represents the viewport's position and size relative to the scrollable extent.

The scrollbar may have an interaction lane along the scroll axis. The lane may be visible as a track or visually invisible.

# Look

By default, the scrollbar is hidden.

When the owning viewport receives pointer movement, active scrolling, or direct scrollbar interaction, the scrollbar fades into view if the viewport has overflow.

After a short inactivity delay, the scrollbar fades out.

The thumb size reflects the visible viewport's proportion of the scrollable extent, within the widget's minimum thumb size.

The lane or track may remain visually invisible. A thumb-only scrollbar is still a scrollbar when it provides position feedback and direct manipulation.

# States

Hidden, fading-in, visible, hover, dragging, fading-out, disabled, and inactive.

# Interaction

Pointer movement over an overflowed viewport reveals the scrollbar.

Wheel, touchpad, touch, keyboard, or programmatic scrolling acts on the routed scrollable viewport. The scrollbar reflects the resulting scroll position.

Dragging the thumb scrolls the viewport along the scrollbar axis.

Dragging preserves the pointer grab offset within the thumb until pointer release or cancellation.

Clicking the lane outside the thumb scrolls by one viewport page toward the click.

Keyboard scrolling commands act on the scrollable viewport selected by focus or shell routing, not on scrollbar chrome.

Disabled or inactive scrollbars do not accept direct manipulation.

# Layout

The scrollbar is aligned to the scroll container edge for its axis, and the thumb remains inside the scrollbar lane bounds.

The scrollbar does not change content layout when fading in or out unless the owning project explicitly defines reserved scrollbar space.

The owning scroll surface owns viewport routing, scroll extent semantics, and scroll-state callbacks.

The CSS block defines axis-specific edge placement and thumb dimensions.

Spec CSS:

```css
.scrollbar {
  position: absolute;
  box-sizing: border-box;
  opacity: var(--opacity);
  transition-property: opacity;
  transition-duration: var(--fade-duration);
}

.scrollbar[data-state~="hidden"] {
  opacity: var(--opacity);
}

.scrollbar[data-state~="fading-in"] {
  opacity: var(--opacity);
}

.scrollbar[data-state~="visible"] {
  opacity: var(--opacity);
}

.scrollbar[data-state~="fading-out"] {
  opacity: var(--opacity);
}

.scrollbar[data-state~="disabled"] {
  opacity: var(--opacity);
}

.scrollbar[data-state~="inactive"] {
  opacity: var(--opacity);
}

.scrollbar[data-variant~="vertical"] {
  inline-size: var(--lane-size);
  inset-block: var(--edge-inset);
  inset-inline-end: var(--edge-inset);
}

.scrollbar[data-variant~="horizontal"] {
  block-size: var(--lane-size);
  inset-inline: var(--edge-inset);
  inset-block-end: var(--edge-inset);
}

.scrollbar__lane {
  inline-size: 100%;
  block-size: 100%;
  background: var(--background);
}

.scrollbar__thumb {
  border-radius: var(--radius);
  background: var(--background);
}

.scrollbar__thumb[data-variant~="vertical"] {
  inline-size: var(--size);
  min-block-size: var(--min-length);
}

.scrollbar__thumb[data-variant~="horizontal"] {
  block-size: var(--size);
  min-inline-size: var(--min-length);
}

.scrollbar__thumb[data-state~="hover"] {
  background: var(--background);
}

.scrollbar__thumb[data-state~="dragging"] {
  background: var(--background);
}
```

# Variants

Vertical, horizontal, overlay, reserved-space, thumb-only, visible-track, disabled, and inactive.

Default variant: overlay thumb-only.

# UI Roles

```css
.scrollbar {
  --lane-size: 10px;
  --edge-inset: 2px;
  --fade-duration: 120ms;
  --inactivity-delay: 800ms;
  --opacity: 0;
}

.scrollbar[data-state~="hidden"] {
  --opacity: 0;
}

.scrollbar[data-state~="fading-in"] {
  --opacity: 1;
}

.scrollbar[data-state~="visible"] {
  --opacity: 1;
}

.scrollbar[data-state~="fading-out"] {
  --opacity: 0;
}

.scrollbar[data-state~="disabled"] {
  --opacity: 0;
}

.scrollbar[data-state~="inactive"] {
  --opacity: 0.4;
}

.scrollbar__lane {
  --background: transparent;
}

.scrollbar__thumb {
  --size: 6px;
  --min-length: 24px;
  --radius: 999px;
  --background: #94a3b8;
}

.scrollbar__thumb[data-state~="hover"] {
  --background: #64748b;
}

.scrollbar__thumb[data-state~="dragging"] {
  --background: #475569;
}
```
