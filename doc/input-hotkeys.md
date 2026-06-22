# Shared Text Input Interaction Contract

This document defines baseline text-input keyboard, pointer, selection, clipboard, and undo/redo behavior for Beryl-owned text fields. Feature-specific field behavior belongs in the owning feature design doc.

This document does not own GUI slots, feature GUI composition, reusable widget anatomy, or widget visual contracts. Those live under `doc/gui/` and linked feature `gui.md` files.

All requirements here are extensions of, and subject to, root `doc/design.md`.

Windows-first behavior is the canonical target for this contract.

## Feature Input Authority

- Conversation composer behavior, including `Enter` submission, `Shift+Enter` newline, `Alt+Up`/`Alt+Down` history, image marker atoms, transcript quote insertion, and focused `Ctrl+Up`/`Ctrl+Down` transcript navigation, is defined in `doc/features/composer/design.md`.
- Transcript selection, quote popup, Markdown-preserving copy, and transcript context-menu behavior are defined in `doc/features/transcript/design.md`.
- Settings text fields, multiline settings fields, and generic color input behavior are defined in `doc/features/settings/design.md`.
- Column browser keyboard behavior is defined in `doc/gui/widgets/column-browser/spec.md` and the owning feature docs.

## Scope

- This contract applies to GUI-owned text input fields unless a feature design explicitly overrides a higher-level command.
- This contract defines user-visible editing behavior, not a required implementation technique.
- Platform-native input services such as IME composition, dead keys, and clipboard integration remain in scope and must not be broken by application-defined hotkeys.

## Shared Editing Semantics

- Text-editing behavior must be reusable across the application rather than reimplemented independently per screen.
- Character-wise caret movement and deletion operate on Unicode grapheme boundaries rather than raw bytes.
- Word-wise caret movement and deletion use one shared word-boundary policy across the application.
- When a selection exists, typed text, paste, Backspace, and Delete replace the selected range unless the field is read-only.
- If a field owns atomic non-text items, caret movement, selection endpoints, deletion, and replacement snap around each item rather than into its internal marker representation.
- Unmodified navigation commands collapse an existing selection toward the direction of travel instead of extending it.
- Read-only fields preserve navigation, caret movement, and selection behavior, but do not permit destructive edits, cut, or paste.

## Keyboard Navigation

- `Left` moves the caret one grapheme left.
- `Right` moves the caret one grapheme right.
- `Shift+Left` extends or shrinks selection one grapheme left.
- `Shift+Right` extends or shrinks selection one grapheme right.
- In multiline fields, `Up` moves the caret to the nearest reachable position on the previous visual line.
- In multiline fields, `Down` moves the caret to the nearest reachable position on the next visual line.
- In multiline fields, `Shift+Up` extends or shrinks selection to the nearest reachable position on the previous visual line.
- In multiline fields, `Shift+Down` extends or shrinks selection to the nearest reachable position on the next visual line.
- `Ctrl+Up`, `Ctrl+Down`, `Alt+Up`, and `Alt+Down` are not shared text-editing commands in the reusable input layer. Feature-specific fields may reserve them for higher-level commands only when their feature contract defines that behavior.
- `Ctrl+Left` moves the caret to the previous word boundary.
- `Ctrl+Right` moves the caret to the next word boundary.
- `Ctrl+Shift+Left` extends or shrinks selection to the previous word boundary.
- `Ctrl+Shift+Right` extends or shrinks selection to the next word boundary.
- `Home` moves the caret to the start of the current logical line.
- `End` moves the caret to the end of the current logical line.
- `Shift+Home` selects from the caret to the start of the current logical line.
- `Shift+End` selects from the caret to the end of the current logical line.
- `Ctrl+Home` moves the caret to the start of the field buffer.
- `Ctrl+End` moves the caret to the end of the field buffer.
- `Ctrl+Shift+Home` selects from the caret to the start of the field buffer.
- `Ctrl+Shift+End` selects from the caret to the end of the field buffer.

## Keyboard Editing

- `Backspace` deletes the selected range, or the grapheme immediately before the caret when no selection exists.
- `Delete` deletes the selected range, or the grapheme immediately after the caret when no selection exists.
- `Ctrl+Backspace` deletes from the caret to the previous word boundary when no selection exists, and otherwise deletes the selected range.
- `Ctrl+Delete` deletes from the caret to the next word boundary when no selection exists, and otherwise deletes the selected range.
- `Ctrl+A` selects the entire field buffer.
- `Ctrl+C` copies the selected range to the system clipboard.
- `Ctrl+Insert` copies the selected range to the system clipboard.
- `Ctrl+X` cuts the selected range to the system clipboard.
- `Shift+Delete` cuts the selected range to the system clipboard.
- `Ctrl+V` pastes system clipboard text at the caret or replaces the current selection.
- `Shift+Insert` pastes system clipboard text at the caret or replaces the current selection.
- `Ctrl+Z` undoes the most recent edit operation in the focused field.
- `Ctrl+Y` redoes the most recently undone edit operation.
- `Ctrl+Shift+Z` may be accepted as a redo alias, but `Ctrl+Y` remains the canonical Windows redo binding.

## Pointer Interaction

- Primary-button click places the caret at the clicked position.
- `Shift` plus primary-button click extends selection from the current anchor to the clicked position.
- Primary-button drag updates selection continuously as the pointer moves.
- Double-click selects the word under the pointer.
- Triple-click selects the current line in multiline fields and the entire field value in single-line fields.

## Field-Type Rules

- In single-line fields, line-based commands treat the entire field as one line.
- In single-line fields, pasted newline characters are normalized into non-line-breaking spacing rather than creating multiple lines.
- In multiline fields, line-based commands operate on logical newline-delimited lines.
- Soft wrapping changes visual layout but does not create logical line boundaries for `Home`, `End`, `Shift+Home`, or `Shift+End`.
- In multiline fields, `Enter` inserts a newline unless a higher-level feature contract reserves that keystroke for submission or another command and exposes a clear newline alternative.
- A field that owns atomic non-text items defines each item's copy fallback text and may attach private field-owned clipboard metadata. Plain-text consumers receive only fallback text.
- Private clipboard metadata may reconstruct field-owned atom types only when the owning feature contract permits it and validates that metadata.

## Reuse And Consistency

- Newly added text fields inherit shared keyboard, pointer, selection, clipboard, and undo/redo semantics by default.
- Feature-specific text fields may add higher-level commands but must not silently override or remove baseline editing mechanics unless their feature contract explicitly requires it.
- A feature-specific `Escape` command bound while a text field is focused may dismiss transient feature state only when the feature contract defines that behavior.
- `Escape` must not mutate field buffer, caret, selection, or undo history unless that same feature contract explicitly says so.
- Feature-specific non-editing navigation commands bound while a text field is focused must not change the field caret or selection.
- Feature-specific commands may insert text into an unfocused field only when a feature contract explicitly defines that behavior.
- External draft insertion into a field must update buffer, saved insertion position, and undo history through the same shared editing semantics as ordinary edits, without changing the system clipboard unless the command is explicitly a clipboard command.
