# Name

Canonical name: two-segment split button

Sometimes known as: split command button

# Purpose

Presents one default command and one secondary flyout command as a joined two-segment control while preserving independent activation, focus, and availability.

# References

Contracts:

- beryl-command-geometry
- expected-action-availability

Widgets:

- command button

# Anatomy

The widget consists of one joined root, one text-labeled primary segment, one compact secondary segment, and one internal separator. The secondary segment contains an owner-supplied glyph and accessible name and is the stable anchor for its associated flyout.

Each segment adapts the referenced `command button` semantics. The owner supplies the two labels or glyphs, commands, availability states and explanations, and associated flyout. The widget does not own feature command effects, flyout content, or product eligibility.

# Look

The two segments read as one compact control with one outer silhouette and no gap. The internal separator distinguishes the commands without making them look unrelated.

Hover, pressed, focused, open, attention, and unavailable feedback applies to the affected segment without changing the root geometry. The primary label remains the stronger command identity; the secondary segment remains compact.

# States

Each segment supports normal, hover, pressed, focused, and unavailable states independently. The secondary segment additionally supports closed, open, and attention states.

The root supports fully available, mixed-availability, and fully unavailable states. Fully
unavailable is the aggregate root state exactly when both segments are unavailable. Both
segment-level unavailable states remain present, the secondary segment is not open, and unavailable
or attention treatment never changes segment size or focus order.

# Interaction

Pointer activation or focused Enter and Space invokes only the command assigned to the exact segment. Holding or long-pressing the primary segment does not activate the secondary command or open the associated flyout.

The primary and secondary segments are independent keyboard focus stops in that order. Tab and Shift+Tab follow the surrounding focus order; the widget does not use a composite roving-focus target.

A visible unavailable segment remains focusable for inspection, satisfies `expected-action-availability`, and never invokes through pointer, keyboard, touch, or programmatic acceptance.

When the root is fully unavailable, neither segment invokes. Each segment remains its own focusable
inspection target with its owner-supplied reason; the derived root state introduces no third focus
or activation target.

Secondary-segment activation requests the owner-supplied flyout for that exact segment. While the flyout is open, the same secondary segment remains its stable anchor and carries open state. Dismissal returns focus to that segment unless successful feature behavior moves focus to another window-level target.

# Layout

The root is content-sized within its containing toolbar and uses the shared Beryl command-control height. The primary segment hugs its label and may shrink only when the containing toolbar is constrained. The secondary segment keeps its compact fixed allocation.

The segments share the root border and corner shape. Only the secondary segment paints the internal separator. Focus feedback is inset and does not alter segment or neighboring toolbar geometry.

The associated flyout is positioned and clamped by its own widget and owning overlay contract. Opening it does not resize the split button or its toolbar.

Spec CSS:

```css
.two-segment-split-button {
  display: inline-flex;
  align-items: stretch;
  box-sizing: border-box;
  inline-size: max-content;
  max-inline-size: 100%;
  block-size: var(--height);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  overflow: hidden;
}

.two-segment-split-button__primary,
.two-segment-split-button__secondary {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  block-size: 100%;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  border: 0;
  background: var(--background);
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  white-space: nowrap;
}

.two-segment-split-button__primary {
  flex: 0 1 auto;
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.two-segment-split-button__secondary {
  flex: none;
  inline-size: var(--width);
  border-inline-start: var(--separator-width) solid var(--separator-color);
}

.two-segment-split-button__primary[data-state~="hover"],
.two-segment-split-button__secondary[data-state~="hover"] {
  background: var(--background);
}

.two-segment-split-button__primary[data-state~="pressed"],
.two-segment-split-button__secondary[data-state~="pressed"] {
  background: var(--background);
}

.two-segment-split-button__primary[data-state~="focused"],
.two-segment-split-button__secondary[data-state~="focused"] {
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.two-segment-split-button__secondary[data-state~="open"],
.two-segment-split-button__secondary[data-state~="attention"] {
  background: var(--background);
  color: var(--foreground);
}

.two-segment-split-button__primary[data-state~="unavailable"],
.two-segment-split-button__secondary[data-state~="unavailable"] {
  background: var(--background);
  color: var(--foreground);
  opacity: var(--opacity);
}
```

# Variants

Default variant: text-labeled primary segment with compact glyph-only secondary flyout segment.

# UI Roles

```css
.two-segment-split-button {
  --height: 32px;
  --border-width: 1px;
  --border-color: #cbd5e1;
  --radius: 6px;
  --background: #f8fafc;
  --foreground: #1f2937;
}

.two-segment-split-button__primary {
  --padding-x: 12px;
  --padding-y: 6px;
  --background: #f8fafc;
  --foreground: #1f2937;
  --font-size: 13px;
  --font-weight: 500;
}

.two-segment-split-button__secondary {
  --width: 32px;
  --padding-x: 6px;
  --padding-y: 6px;
  --separator-width: 1px;
  --separator-color: #cbd5e1;
  --background: #f8fafc;
  --foreground: #1f2937;
  --font-size: 13px;
  --font-weight: 500;
}

.two-segment-split-button__primary[data-state~="hover"] {
  --background: #eef2f7;
}

.two-segment-split-button__secondary[data-state~="hover"] {
  --background: #eef2f7;
}

.two-segment-split-button__primary[data-state~="pressed"] {
  --background: #e2e8f0;
}

.two-segment-split-button__secondary[data-state~="pressed"] {
  --background: #e2e8f0;
}

.two-segment-split-button__primary[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #2563eb;
  --ring-offset: -2px;
}

.two-segment-split-button__secondary[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #2563eb;
  --ring-offset: -2px;
}

.two-segment-split-button__secondary[data-state~="open"] {
  --background: #dbeafe;
  --foreground: #1d4ed8;
}

.two-segment-split-button__secondary[data-state~="attention"] {
  --background: #fef3c7;
  --foreground: #92400e;
}

.two-segment-split-button__primary[data-state~="unavailable"] {
  --background: #f1f5f9;
  --foreground: #94a3b8;
  --opacity: 1;
}

.two-segment-split-button__secondary[data-state~="unavailable"] {
  --background: #f1f5f9;
  --foreground: #94a3b8;
  --opacity: 1;
}
```
