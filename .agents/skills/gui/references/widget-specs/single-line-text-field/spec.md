# Name

Canonical name: single-line text field

Sometimes known as: text field, text box, edit field, single-line text input

# Purpose

Accepts and edits one line of text.

# References

N/A

# Anatomy

The single-line text field consists of a bordered field body, optional placeholder text, editable text content, an insertion caret, and optional leading or trailing adornments.

Adornments may include icons, clear buttons, validation indicators, or unit labels when the owning feature requires them.

# Look

The field presents editable single-line text inside a bordered body with placeholder, caret, and optional adornment visuals.

Placeholder text appears only when the field is empty and not showing entered content.

# States

Normal, hover, focused, disabled, readonly, invalid, required, empty, filled, and placeholder-visible.

# Interaction

Typing inserts text at the caret.

The field does not accept newline input. Enter may commit or submit the owning form only when the owning feature defines that behavior.

Horizontal scrolling is implied when the text exceeds the visible width.

The field supports ordinary platform text-editing behavior. On platforms that use Ctrl bindings,
this includes Home and End for line start and line end, Ctrl+Left and Ctrl+Right for previous and
next word, Shift selection extension, Ctrl+A selection, Ctrl+C copy, Ctrl+X cut, Ctrl+V paste,
Ctrl+Z undo, Ctrl+Y redo, Backspace and Delete, Ctrl+Backspace, and Ctrl+Delete. Other platforms use
their equivalent conventional bindings.

Mouse or touch interaction may place the caret, drag-select text, and use platform text selection behavior.

# Layout

The field occupies one editable line and receives its inline size from the owning layout.

The text content is clipped to the field body and scrolls horizontally instead of wrapping.

The CSS block defines the one-line box metrics, padding, clipping, and state visuals.

Spec CSS:

```css
.single-line-text-field {
  display: inline-flex;
  align-items: center;
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
  overflow: hidden;
}

.single-line-text-field__content {
  flex: 1 1 auto;
  min-inline-size: 0;
  white-space: nowrap;
  overflow: hidden;
}

.single-line-text-field__placeholder {
  color: var(--foreground);
}

.single-line-text-field__caret {
  color: var(--foreground);
}

.single-line-text-field__adornment-icon {
  inline-size: var(--size);
  block-size: var(--size);
  color: var(--foreground);
}

.single-line-text-field[data-state~="hover"] {
  border-color: var(--border-color);
}

.single-line-text-field[data-state~="focused"] {
  border-color: var(--border-color);
  outline: var(--ring-width) solid var(--ring-color);
  outline-offset: var(--ring-offset);
}

.single-line-text-field[data-state~="disabled"] {
  background: var(--background);
  color: var(--foreground);
  border-color: var(--border-color);
}

.single-line-text-field[data-state~="readonly"] {
  background: var(--background);
  color: var(--foreground);
}

.single-line-text-field[data-state~="invalid"] {
  border-color: var(--border-color);
  outline-color: var(--ring-color);
}
```

# Variants

Plain, search, password, numeric, readonly, invalid, with leading icon, with trailing action, and with unit label.

Default variant: plain.

# UI Roles

```css
.single-line-text-field {
  --height: 32px;
  --min-width: 160px;
  --padding-x: 10px;
  --padding-y: 6px;
  --gap: 6px;
  --radius: 6px;
  --border-width: 1px;
  --font-size: 13px;
  --background: #ffffff;
  --foreground: #111827;
  --border-color: #cbd5e1;
}

.single-line-text-field__placeholder {
  --foreground: #64748b;
}

.single-line-text-field__caret {
  --foreground: #2563eb;
}

.single-line-text-field__adornment-icon {
  --size: 16px;
  --foreground: #64748b;
}

.single-line-text-field[data-state~="hover"] {
  --border-color: #94a3b8;
}

.single-line-text-field[data-state~="focused"] {
  --border-color: #2563eb;
  --ring-width: 2px;
  --ring-color: #93c5fd;
  --ring-offset: 0px;
}

.single-line-text-field[data-state~="disabled"] {
  --background: #f1f5f9;
  --foreground: #94a3b8;
  --border-color: #cbd5e1;
}

.single-line-text-field[data-state~="readonly"] {
  --background: #f8fafc;
  --foreground: #334155;
}

.single-line-text-field[data-state~="invalid"] {
  --border-color: #dc2626;
  --ring-color: #fecaca;
}
```
