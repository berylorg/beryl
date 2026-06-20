# Name

Canonical name: command button

Sometimes known as: action button, push button

# Purpose

Invokes a discrete command selected by the user.

# Anatomy

The command button consists of a rounded rectangular button body and a centered label.

The label is usually text. A project may add a leading or trailing icon when the icon clarifies the command, but the text label remains the primary identity unless the project defines an icon-only variant.

# Look

The button is a rectangle with rounded corners.

The body hugs the label with horizontal and vertical padding. The body has enough visual weight to read as an interactive control.

The button uses background, border, and text colors to communicate its variant and state.

Hover highlights the button with a visible color, border, or surface change.

Press highlights the button more strongly while activation is held, then returns to the appropriate post-press state when released.

The default variant may have a stronger outline, fill, or accent treatment to indicate that it is the command activated by Enter in the current context.

# States

Normal, hover, pressed, focused, disabled, and loading.

Project variants may also define selected or active states.

# Interaction

Clicking or tapping the button invokes its assigned command when enabled.

When focused, Enter and Space invoke the command unless the owning feature defines a stricter keyboard contract.

When the button is the default command for its context, pressing Enter from the relevant context invokes it.

Disabled buttons do not invoke their command.

# Layout

The button hugs its label by default.

The button may fill available width only when its containing layout explicitly requires it.

The label stays centered within the button body. Text should not overlap icons, borders, or neighboring content.

# Variants

Primary, secondary, default command, destructive, icon-leading, icon-trailing, and full-width.

Default variant: secondary.

# UI Roles

## Root

- `height`: `32px`
- `min-width`: `64px`
- `padding-x`: `12px`
- `padding-y`: `6px`
- `gap`: `6px`
- `radius`: `6px`
- `border-width`: `1px`
- `font-size`: `13px`
- `font-weight`: `500`
- `background`: `#f8fafc`
- `foreground`: `#1f2937`
- `border-color`: `#cbd5e1`

## Parts

### `icon`

- `size`: `16px`
- `foreground`: `currentColor`

## States

### `hover`

- `background`: `#eef2f7`
- `border-color`: `#94a3b8`

### `pressed`

- `background`: `#e2e8f0`
- `border-color`: `#64748b`

### `focused`

- `ring-width`: `2px`
- `ring-color`: `#2563eb`
- `ring-offset`: `2px`

### `disabled`

- `background`: `#f1f5f9`
- `foreground`: `#94a3b8`
- `border-color`: `#cbd5e1`
- `opacity`: `1`
