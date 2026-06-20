# Name

Canonical name: hold-to-confirm button

Sometimes known as: press-and-hold button, hold-to-activate button, safety hold button

# Purpose

Invokes a high-risk or deliberate command only after the user holds activation for a required duration.

# Anatomy

The hold-to-confirm button consists of a rounded rectangular button body, a centered label, and a hold progress fill.

The hold progress fill is drawn inside the button body while activation is held.

The button may include a leading or trailing icon when the icon clarifies the command or risk.

# Look

The button starts with the same general shape as a command button: a rounded rectangle with a centered label and padding.

While the user holds activation, the button background becomes a progress fill bar.

The progress fill grows from the start edge toward the end edge, or according to the project's writing-direction rules, until it reaches full completion.

The label remains readable over the progress fill.

If the hold is cancelled before completion, the progress fill recedes or resets to the normal button background.

When the hold completes, the button shows activation feedback before returning to the appropriate normal or disabled state.

# States

Normal, hover, holding, hold-progress, hold-cancelled, activated, focused, disabled, and failed.

# Interaction

Pointer down or touch start begins the hold timer when the button is enabled.

Holding activation continuously advances the progress fill.

Releasing before the required hold duration cancels activation and resets the progress fill.

Moving the pointer outside the button, losing capture, pressing Escape, or otherwise cancelling the gesture cancels activation unless the owning environment defines a different capture rule.

When the hold duration completes, the button invokes its assigned command once.

Keyboard activation may require holding Space or Enter for the same duration when the project supports keyboard hold behavior.

Disabled buttons do not start hold progress or invoke their command.

# Layout

The button follows command button layout rules for size, padding, and label alignment.

The progress fill is clipped to the button body and does not change the button's layout size.

The label, icon, and progress fill must not overlap in a way that makes the label unreadable.

# Variants

Primary, destructive, icon-leading, full-width, and keyboard-hold-supported.

Default variant: destructive.

# UI Roles

## Root

- `height`: `32px`
- `min-width`: `112px`
- `padding-x`: `12px`
- `padding-y`: `6px`
- `gap`: `6px`
- `radius`: `6px`
- `border-width`: `1px`
- `font-size`: `13px`
- `font-weight`: `600`
- `background`: `#fee2e2`
- `foreground`: `#991b1b`
- `border-color`: `#fecaca`

## Parts

### `icon`

- `size`: `16px`
- `foreground`: `currentColor`

### `progress-fill`

- `background`: `#dc2626`
- `foreground`: `#ffffff`
- `transition-duration`: `120ms`

#### States

##### `hold-cancelled`

- `opacity`: `0`
- `transition-duration`: `120ms`

## States

### `hover`

- `background`: `#fecaca`
- `border-color`: `#fca5a5`

### `holding`

- `background`: `#fee2e2`
- `foreground`: `#ffffff`
- `border-color`: `#dc2626`

### `activated`

- `background`: `#dc2626`
- `foreground`: `#ffffff`
- `border-color`: `#b91c1c`

### `focused`

- `ring-width`: `2px`
- `ring-color`: `#dc2626`
- `ring-offset`: `2px`

### `disabled`

- `background`: `#f1f5f9`
- `foreground`: `#94a3b8`
- `border-color`: `#cbd5e1`
- `opacity`: `1`

### `failed`

- `background`: `#fef2f2`
- `foreground`: `#991b1b`
- `border-color`: `#ef4444`
