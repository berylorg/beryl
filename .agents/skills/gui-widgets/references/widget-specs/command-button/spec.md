# Name

Canonical name: command button

Sometimes known as: action button, push button

# Purpose

Invokes a discrete command selected by the user.

# Anatomy

The command button consists of a rounded rectangular button body and a centered label.

The label is usually text. A project may add a leading or trailing icon when the icon clarifies the command, but the text label remains the primary identity unless the project defines an icon-only variant.

# Look

The button reads as a compact rectangular command control with visible state feedback.

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

The button has no default minimum width beyond its label, icon, gap, border, and padding.

The button may fill available width only when its containing layout explicitly requires it.

Buttons whose visible text comes from a known finite cycling or toggle label set may reserve width for the longest label in that set.

The label stays centered within the button body. Text should not overlap icons, borders, or neighboring content.

The CSS block defines the default inline sizing, padding, gap, clipping, and full-width variant.

Spec CSS:

```css
.command-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  block-size: var(--height);
  inline-size: max-content;
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

.command-button[data-state~="hover"] {
  background: var(--background);
  border-color: var(--border-color);
}

.command-button[data-state~="pressed"] {
  background: var(--background);
  border-color: var(--border-color);
}

.command-button[data-state~="focused"] {
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.command-button[data-state~="disabled"] {
  background: var(--background);
  color: var(--foreground);
  border-color: var(--border-color);
  opacity: var(--opacity);
}

.command-button[data-variant="full-width"] {
  inline-size: 100%;
}

.command-button__icon {
  inline-size: var(--size);
  block-size: var(--size);
  color: var(--foreground);
}
```

# Variants

Primary, secondary, default command, destructive, icon-leading, icon-trailing, and full-width.

Default variant: secondary.

# UI Roles

```css
.command-button {
  --height: 32px;
  --padding-x: 12px;
  --padding-y: 6px;
  --gap: 6px;
  --radius: 6px;
  --border-width: 1px;
  --font-size: 13px;
  --font-weight: 500;
  --background: #f8fafc;
  --foreground: #1f2937;
  --border-color: #cbd5e1;
}

.command-button__icon {
  --size: 16px;
  --foreground: currentColor;
}

.command-button[data-state~="hover"] {
  --background: #eef2f7;
  --border-color: #94a3b8;
}

.command-button[data-state~="pressed"] {
  --background: #e2e8f0;
  --border-color: #64748b;
}

.command-button[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #2563eb;
  --ring-offset: 2px;
}

.command-button[data-state~="disabled"] {
  --background: #f1f5f9;
  --foreground: #94a3b8;
  --border-color: #cbd5e1;
  --opacity: 1;
}
```
