# Name

Canonical name: segmented status bar

Sometimes known as: status bar, status line, segmented status line, readout strip

# Purpose

Shows compact status or readout information in a segmented strip at the bottom of a panel or window.

# References

Contracts:

- disabled-command-tooltip

# Anatomy

The segmented status bar consists of a horizontal strip divided into segments. When the same pattern is used primarily for frequently updated live values, it may be called a readout strip.

Segments are separated visually by vertical dividers.

Each segment may contain one or more key-value pairs. A key-value pair contains a label-like key and a value, separated by spacing.

A segment may be passive text, a readout selector, an action-menu segment, or a direct-action segment.

# Look

The segmented status bar reads as a compact status or readout strip attached to a panel or window edge.

A segment may display several short readout pairs in one compact run.

Interactive segments show hover, pressed, focused, and open feedback while retaining the compact status-bar appearance.

# States

Normal, hover, pressed, focused, disabled, active, open, and truncated.

Passive segments use normal and truncated states only unless the project defines additional visual states.

# Interaction

Passive segments do not respond to activation.

Interactive segments are controls. Clicking, tapping, or keyboard-activating an enabled interactive segment performs the segment's assigned behavior.

A readout selector opens a selector, dropdown, or flyout for changing the value represented by the segment.

An action-menu segment opens a command menu or action menu related to the displayed state.

A direct-action segment invokes one command without opening a transient panel.

The segmented status bar does not define the internal look or behavior of opened menus, selectors, dropdowns, or flyouts.

When a segment's associated transient panel is open, that segment enters the open or active state.

Disabled interactive command-like segments must satisfy `disabled-command-tooltip`.

# Layout

The segmented status bar sits at the bottom of its owning panel or window.

Segments are laid out horizontally.

Segment width may hug content, use fixed widths, or share remaining width according to the owning layout.

Content that does not fit may truncate inside its segment. Segment content should not overlap neighboring segments or dividers.

The CSS block defines the horizontal strip layout, segment box behavior, divider geometry, and truncation.

Spec CSS:

```css
.segmented-status-bar {
  display: flex;
  align-items: stretch;
  box-sizing: border-box;
  block-size: var(--height);
  background: var(--background);
  color: var(--foreground);
  border-block-start: var(--border-block-start-width) solid var(--border-block-start-color);
  font-size: var(--font-size);
  overflow: hidden;
}

.segmented-status-bar__segment {
  display: inline-flex;
  align-items: center;
  min-inline-size: 0;
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  gap: var(--gap);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.segmented-status-bar__segment[data-state~="hover"] {
  background: var(--background);
}

.segmented-status-bar__segment[data-state~="pressed"] {
  background: var(--background);
}

.segmented-status-bar__segment[data-state~="focused"] {
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.segmented-status-bar__segment[data-state~="open"] {
  background: var(--background);
  color: var(--foreground);
}

.segmented-status-bar__segment[data-state~="disabled"] {
  color: var(--foreground);
}

.segmented-status-bar__divider {
  inline-size: var(--width);
  background: var(--background);
}

.segmented-status-bar__key {
  color: var(--foreground);
  font-weight: var(--font-weight);
}

.segmented-status-bar__value {
  color: var(--foreground);
  font-weight: var(--font-weight);
}
```

# Variants

Passive, readout strip, readout selector segment, action-menu segment, direct-action segment, fixed-width segment, flexible segment, and truncated segment.

Default variant: passive compact status bar.

# UI Roles

```css
.segmented-status-bar {
  --height: 24px;
  --background: #f8fafc;
  --foreground: #334155;
  --border-block-start-width: 1px;
  --border-block-start-color: #cbd5e1;
  --font-size: 12px;
}

.segmented-status-bar__segment {
  --padding-x: 8px;
  --padding-y: 0px;
  --gap: 6px;
}

.segmented-status-bar__segment[data-state~="hover"] {
  --background: #eef2f7;
}

.segmented-status-bar__segment[data-state~="pressed"] {
  --background: #e2e8f0;
}

.segmented-status-bar__segment[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #2563eb;
  --ring-offset: -2px;
}

.segmented-status-bar__segment[data-state~="open"] {
  --background: #e2e8f0;
  --foreground: #0f172a;
}

.segmented-status-bar__segment[data-state~="disabled"] {
  --foreground: #94a3b8;
}

.segmented-status-bar__divider {
  --width: 1px;
  --background: #cbd5e1;
}

.segmented-status-bar__key {
  --foreground: #64748b;
  --font-weight: 400;
}

.segmented-status-bar__value {
  --foreground: #0f172a;
  --font-weight: 500;
}
```
