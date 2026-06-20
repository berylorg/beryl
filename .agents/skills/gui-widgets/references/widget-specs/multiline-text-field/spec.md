# Name

Canonical name: multiline text field

Sometimes known as: text area, multiline text box, multiline text input, multiline edit field

# Purpose

Accepts and edits text that may contain multiple lines.

# Anatomy

The multiline text field consists of a bordered field body, optional placeholder text, editable text content, an insertion caret, a scroll container, and optional resize affordance.

The field may include optional leading or trailing adornments only when they do not interfere with text editing.

# Look

The field body surrounds the text with padding and a cosmetic border.

Text starts at the top of the editable area.

Text wraps within the field width. Horizontal scrolling is not used for ordinary text entry.

The field may grow vertically as new lines are inserted, up to a fixed maximum height.

After the maximum height is reached, overflowing text scrolls vertically.

When focused, the field shows a visible focus treatment through border, outline, or surface change.

# States

Normal, hover, focused, disabled, readonly, invalid, required, empty, filled, placeholder-visible, scrollable, and resized.

# Interaction

Typing inserts text at the caret.

Enter inserts a newline unless the owning feature defines a command-specific override.

The field supports ordinary Windows multiline editing behavior, including Home and End for line start and line end, Ctrl+Home and Ctrl+End for document start and document end, Ctrl+Left and Ctrl+Right for previous and next word, Shift selection extension, Ctrl+A selection, Ctrl+C copy, Ctrl+X cut, Ctrl+V paste, Ctrl+Z undo, Ctrl+Y redo, Backspace and Delete, Ctrl+Backspace, Ctrl+Delete, Page Up, and Page Down.

Mouse or touch interaction may place the caret, drag-select text, and use platform text selection behavior.

If manual resizing is enabled, dragging the resize affordance changes the visible height within the allowed minimum and maximum heights.

# Layout

The field may start at a fixed height or an auto height.

The field grows vertically from inserted newlines until reaching its maximum height.

The field does not scroll horizontally for ordinary text entry. Text wraps to fit the available width.

When content exceeds the maximum height, vertical scrolling is activated inside the field.

# Variants

Fixed-height, auto-growing, manually resizable, readonly, invalid, and code-like.

Default variant: fixed-height.

# UI Roles

## Root

- `min-height`: `88px`
- `max-height`: `240px`
- `min-width`: `240px`
- `padding-x`: `10px`
- `padding-y`: `8px`
- `radius`: `6px`
- `border-width`: `1px`
- `font-size`: `13px`
- `line-height`: `18px`
- `background`: `#ffffff`
- `foreground`: `#111827`
- `border-color`: `#cbd5e1`

## Parts

### `placeholder`

- `foreground`: `#64748b`

### `caret`

- `foreground`: `#2563eb`

### `resize-affordance`

- `size`: `12px`
- `foreground`: `#94a3b8`

## States

### `hover`

- `border-color`: `#94a3b8`

### `focused`

- `border-color`: `#2563eb`
- `ring-width`: `2px`
- `ring-color`: `#93c5fd`
- `ring-offset`: `0px`

### `disabled`

- `background`: `#f1f5f9`
- `foreground`: `#94a3b8`
- `border-color`: `#cbd5e1`

### `readonly`

- `background`: `#f8fafc`
- `foreground`: `#334155`

### `invalid`

- `border-color`: `#dc2626`
- `ring-color`: `#fecaca`
