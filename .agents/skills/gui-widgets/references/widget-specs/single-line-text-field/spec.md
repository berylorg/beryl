# Name

Canonical name: single-line text field

Sometimes known as: text field, text box, edit field, single-line text input

# Purpose

Accepts and edits one line of text.

# Anatomy

The single-line text field consists of a bordered field body, optional placeholder text, editable text content, an insertion caret, and optional leading or trailing adornments.

Adornments may include icons, clear buttons, validation indicators, or unit labels when the owning feature requires them.

# Look

The field has a fixed single-line height.

The field body surrounds the text with padding and a cosmetic border.

Text is vertically centered within the line box. Placeholder text appears only when the field is empty and not showing entered content.

When focused, the field shows a visible focus treatment through border, outline, or surface change.

Text overflows horizontally. The field scrolls horizontally as needed to keep the caret and edited text visible.

# States

Normal, hover, focused, disabled, readonly, invalid, required, empty, filled, and placeholder-visible.

# Interaction

Typing inserts text at the caret.

The field does not accept newline input. Enter may commit or submit the owning form only when the owning feature defines that behavior.

Horizontal scrolling is implied when the text exceeds the visible width.

The field supports ordinary Windows single-line editing behavior, including Home and End for line start and line end, Ctrl+Left and Ctrl+Right for previous and next word, Shift selection extension, Ctrl+A selection, Ctrl+C copy, Ctrl+X cut, Ctrl+V paste, Ctrl+Z undo, Ctrl+Y redo, Backspace and Delete, Ctrl+Backspace, and Ctrl+Delete.

Mouse or touch interaction may place the caret, drag-select text, and use platform text selection behavior.

# Layout

The field has a fixed height based on one text line plus vertical padding.

The field may have fixed, minimum, maximum, or fill width according to its containing layout.

The text content is clipped to the field body and scrolls horizontally instead of wrapping.

# Variants

Plain, search, password, numeric, readonly, invalid, with leading icon, with trailing action, and with unit label.

Default variant: plain.

# UI Roles

## Root

- `height`: `32px`
- `min-width`: `160px`
- `padding-x`: `10px`
- `padding-y`: `6px`
- `gap`: `6px`
- `radius`: `6px`
- `border-width`: `1px`
- `font-size`: `13px`
- `background`: `#ffffff`
- `foreground`: `#111827`
- `border-color`: `#cbd5e1`

## Parts

### `placeholder`

- `foreground`: `#64748b`

### `caret`

- `foreground`: `#2563eb`

### `adornment-icon`

- `size`: `16px`
- `foreground`: `#64748b`

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
