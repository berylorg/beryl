# Name

Canonical name: segmented status bar

Sometimes known as: status bar, status line, segmented status line

# Purpose

Shows compact status information in a segmented strip at the bottom of a panel or window.

# Anatomy

The segmented status bar consists of a horizontal strip divided into segments.

Segments are separated visually by vertical dividers.

Each segment may contain one or more key-value pairs. A key-value pair contains a label-like key and a value, separated by spacing.

A segment may be passive text, or it may be an interactive segment button that opens an anchored context menu.

# Look

The segmented status bar is a narrow horizontal strip attached to the bottom edge of a panel or window.

Segments are visually separated by vertical bars, divider lines, or equivalent separators.

Within a segment, key-value pairs are arranged inline with compact spacing.

For example, one segment may display `Context 79%  5h 100%  Weekly 85%`.

Interactive segment buttons show hover, pressed, and focused feedback while retaining the compact status-bar appearance.

# States

Normal, hover, pressed, focused, disabled, active, open, and truncated.

Passive segments use normal and truncated states only unless the project defines additional visual states.

# Interaction

Passive segments do not respond to activation.

Interactive segments act as buttons. Clicking, tapping, or keyboard-activating an enabled interactive segment opens its associated anchored context menu.

The segmented status bar does not define the internal look or behavior of the anchored context menu. It only defines that a segment may open one.

When a segment's anchored context menu is open, that segment enters the open or active state.

# Layout

The segmented status bar sits at the bottom of its owning panel or window.

Segments are laid out horizontally.

Segment width may hug content, use fixed widths, or share remaining width according to the owning layout.

Content that does not fit may truncate inside its segment. Segment content should not overlap neighboring segments or dividers.

# Variants

Passive, interactive segment, fixed-width segment, flexible segment, and truncated segment.

Default variant: passive compact status bar.

# UI Roles

## Root

- `height`: `24px`
- `background`: `#f8fafc`
- `foreground`: `#334155`
- `border-top-width`: `1px`
- `border-top-color`: `#cbd5e1`
- `font-size`: `12px`

## Parts

### `segment`

- `padding-x`: `8px`
- `padding-y`: `0px`
- `gap`: `6px`

#### States

##### `hover`

- `background`: `#eef2f7`

##### `pressed`

- `background`: `#e2e8f0`

##### `focused`

- `ring-width`: `2px`
- `ring-color`: `#2563eb`
- `ring-offset`: `-2px`

##### `open`

- `background`: `#e2e8f0`
- `foreground`: `#0f172a`

##### `disabled`

- `foreground`: `#94a3b8`

### `divider`

- `width`: `1px`
- `background`: `#cbd5e1`

### `key`

- `foreground`: `#64748b`
- `font-weight`: `400`

### `value`

- `foreground`: `#0f172a`
- `font-weight`: `500`
