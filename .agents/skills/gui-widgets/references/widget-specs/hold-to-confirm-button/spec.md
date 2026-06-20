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

The button uses command-button visual structure plus a progress fill that communicates held activation.

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

The button follows command button layout rules.

The progress fill is clipped to the button body and does not change the button's layout size.

The label, icon, and progress fill must not overlap in a way that makes the label unreadable.

The CSS block defines clipping, progress-fill sizing, and state styling.

Spec CSS:

```css
.hold-to-confirm-button {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  block-size: var(--height);
  min-inline-size: var(--min-width);
  padding-inline: var(--padding-x);
  padding-block: var(--padding-y);
  gap: var(--gap);
  border: var(--border-width) solid var(--border-color);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  font-size: var(--font-size);
  font-weight: var(--font-weight);
  white-space: nowrap;
  overflow: hidden;
}

.hold-to-confirm-button__progress-fill {
  position: absolute;
  inset-block: 0;
  inset-inline-start: 0;
  inline-size: calc(var(--hold-progress) * 100%);
  background: var(--background);
  color: var(--foreground);
  transition-duration: var(--transition-duration);
}

.hold-to-confirm-button__icon {
  inline-size: var(--size);
  block-size: var(--size);
  color: var(--foreground);
}

.hold-to-confirm-button[data-state~="hover"] {
  background: var(--background);
  border-color: var(--border-color);
}

.hold-to-confirm-button[data-state~="holding"] {
  background: var(--background);
  color: var(--foreground);
  border-color: var(--border-color);
}

.hold-to-confirm-button[data-state~="activated"] {
  background: var(--background);
  color: var(--foreground);
  border-color: var(--border-color);
}

.hold-to-confirm-button[data-state~="focused"] {
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.hold-to-confirm-button[data-state~="disabled"] {
  background: var(--background);
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}

.hold-to-confirm-button[data-state~="failed"] {
  background: var(--background);
  color: var(--foreground);
  border-color: var(--border-color);
}

.hold-to-confirm-button__progress-fill[data-state~="hold-cancelled"] {
  opacity: var(--opacity);
  transition-duration: var(--transition-duration);
}
```

# Variants

Primary, destructive, icon-leading, full-width, and keyboard-hold-supported.

Default variant: destructive.

# UI Roles

```css
.hold-to-confirm-button {
  --height: 32px;
  --min-width: 112px;
  --padding-x: 12px;
  --padding-y: 6px;
  --gap: 6px;
  --radius: 6px;
  --border-width: 1px;
  --font-size: 13px;
  --font-weight: 600;
  --background: #fee2e2;
  --foreground: #991b1b;
  --border-color: #fecaca;
}

.hold-to-confirm-button__icon {
  --size: 16px;
  --foreground: currentColor;
}

.hold-to-confirm-button__progress-fill {
  --background: #dc2626;
  --foreground: #ffffff;
  --transition-duration: 120ms;
}

.hold-to-confirm-button__progress-fill[data-state~="hold-cancelled"] {
  --opacity: 0;
  --transition-duration: 120ms;
}

.hold-to-confirm-button[data-state~="hover"] {
  --background: #fecaca;
  --border-color: #fca5a5;
}

.hold-to-confirm-button[data-state~="holding"] {
  --background: #fee2e2;
  --foreground: #ffffff;
  --border-color: #dc2626;
}

.hold-to-confirm-button[data-state~="activated"] {
  --background: #dc2626;
  --foreground: #ffffff;
  --border-color: #b91c1c;
}

.hold-to-confirm-button[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #dc2626;
  --ring-offset: 2px;
}

.hold-to-confirm-button[data-state~="disabled"] {
  --background: #f1f5f9;
  --foreground: #94a3b8;
  --border-color: #cbd5e1;
  --opacity: 1;
}

.hold-to-confirm-button[data-state~="failed"] {
  --background: #fef2f2;
  --foreground: #991b1b;
  --border-color: #ef4444;
}
```
