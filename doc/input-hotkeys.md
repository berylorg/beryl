# Shared Text Input Interaction Contract

This document defines the shared text-input keyboard and closely coupled pointer behavior referenced by `doc/design.md`.

All requirements in this document are extensions of, and subject to, the constraints in `doc/design.md`.

Windows-first behavior is the canonical target for this contract.

## Scope

- This contract applies to every GUI-owned text input field in the application, including startup-path fields, the conversation composer, and later settings fields.
- This contract defines user-visible editing behavior, not a required implementation technique.
- Platform-native input services such as IME composition, dead keys, and clipboard integration remain in scope and must not be broken by application-defined hotkeys.

## Shared Editing Semantics

- Text-editing behavior must be reusable across the application rather than reimplemented independently per screen.
- Character-wise caret movement and deletion operate on Unicode grapheme boundaries rather than raw bytes.
- Word-wise caret movement and deletion operate on one shared word-boundary policy used consistently across the application.
- When a selection exists, typed text, paste, Backspace, and Delete replace the selected range unless a field is explicitly read-only.
- If a screen-specific field owns an atomic non-text item, caret movement, selection endpoints, deletion, and replacement snap around the item rather than into its internal marker text.
- Unmodified navigation commands collapse an existing selection toward the direction of travel instead of extending it.
- Read-only text fields preserve navigation, caret movement, and selection behavior, but do not permit destructive edits, cut, or paste.

## Keyboard Navigation

- `Left` moves the caret one grapheme to the left.
- `Right` moves the caret one grapheme to the right.
- `Shift+Left` extends or shrinks the selection one grapheme to the left.
- `Shift+Right` extends or shrinks the selection one grapheme to the right.
- In multiline fields, `Up` moves the caret to the nearest reachable position on the previous visual line.
- In multiline fields, `Down` moves the caret to the nearest reachable position on the next visual line.
- In multiline fields, `Shift+Up` extends or shrinks the selection to the nearest reachable position on the previous visual line.
- In multiline fields, `Shift+Down` extends or shrinks the selection to the nearest reachable position on the next visual line.
- `Ctrl+Up` and `Ctrl+Down` are not shared text-editing commands in the reusable input layer.
- `Alt+Up` and `Alt+Down` are not shared text-editing commands in the reusable input layer; screen-specific fields may reserve them for higher-level history or navigation commands only when their product contract defines that behavior.
- `Ctrl+Left` moves the caret to the previous word boundary.
- `Ctrl+Right` moves the caret to the next word boundary.
- `Ctrl+Shift+Left` extends or shrinks the selection to the previous word boundary.
- `Ctrl+Shift+Right` extends or shrinks the selection to the next word boundary.
- `Home` moves the caret to the start of the current line.
- `End` moves the caret to the end of the current line.
- `Shift+Home` selects from the caret to the start of the current line.
- `Shift+End` selects from the caret to the end of the current line.
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
- In the conversation composer only, paste commands may consume image clipboard content and insert an inline image marker at the caret or replace the selected range. Other text fields remain text-only paste targets unless their own product contract explicitly opts into non-text content.
- `Ctrl+Z` undoes the most recent edit operation in the focused field.
- `Ctrl+Y` redoes the most recently undone edit operation in the focused field.
- `Ctrl+Shift+Z` may be accepted as a redo alias, but `Ctrl+Y` remains the canonical Windows redo binding.

## Pointer Interaction

- Primary-button click places the caret at the clicked position.
- `Shift` plus primary-button click extends the selection from the current anchor to the clicked position.
- Primary-button drag updates the selection continuously as the pointer moves.
- Double-click selects the word under the pointer.
- Triple-click selects the current line in multiline fields and the entire field value in single-line fields.

## Field-Type Rules

- In single-line fields, line-based commands treat the entire field as one line.
- In single-line fields, pasted newline characters are normalized into non-line-breaking spacing rather than creating multiple lines.
- In multiline fields, line-based commands operate on logical newline-delimited lines.
- Soft wrapping changes visual layout but does not create line boundaries for `Home`, `End`, `Shift+Home`, or `Shift+End`.
- In multiline fields, `Enter` inserts a newline unless a higher-level screen contract explicitly reserves that keystroke for submission and exposes a clear alternative for newline insertion.
- The conversation composer is such a higher-level field: its screen contract owns `Enter` submission, including queued and active-turn steering submissions, while `Shift+Enter` remains the explicit newline path.
- The conversation composer owns `Alt+Up` and `Alt+Down` as higher-level history browsing commands while that field is focused. These commands replace the draft only through composer-owned behavior, must not run during active IME or marked-text composition, and are no-ops in other text fields unless those fields define their own product contract.
- The conversation composer may contain atomic non-text draft items such as pasted image markers. Baseline caret, selection, deletion, cut, paste, undo, and redo behavior applies to those items as if each marker occupied one indivisible draft position.
- Clipboard copy or cut serializes selected non-text draft items through their field-defined copy text. A selected composer image marker copies as `[Image A]` for label `A`, so copied draft text remains meaningful outside the application.
- A field that owns non-text draft items may additionally attach field-owned clipboard metadata to the copied text. Baseline text-input behavior remains domain-neutral: plain-text consumers receive only the field-defined copy text, while the owning field may use metadata on later paste to reconstruct its own atom types.
- In the conversation composer, Beryl-authored clipboard metadata may restore copied image markers as atomic draft items. Visible clipboard text shaped like `[Image A]` without valid Beryl metadata remains ordinary pasted text and must not create an image attachment.
- Pasting a copied composer image marker in the same label scope keeps it as another reference to the same image. Cutting and then pasting a selected marker is the user-visible way to move that image reference within draft text.

## Reuse And Consistency

- The application must expose shared baseline text-input behavior so that newly added text fields inherit the same keyboard, pointer, selection, clipboard, and undo or redo semantics by default.
- Screen-specific text fields may add higher-level commands, but must not silently override or remove the baseline editing mechanics defined here unless a separate product contract explicitly requires it.
- A screen-specific `Escape` command bound while a text field is focused may dismiss transient screen state only when the product contract defines that behavior. It must not mutate the field buffer, caret, selection, or undo history unless that same product contract explicitly says so.
- A screen-specific non-editing navigation command bound while a text field is focused must not change the field's caret or selection.
- Screen-specific commands may insert text into a field that is not currently focused only when a product contract explicitly defines that behavior, such as transcript quote insertion into the conversation draft.
- External draft insertion into a field must update that field's buffer, saved insertion position, and undo history through the same shared editing semantics as an ordinary edit, without changing the system clipboard unless the command is explicitly a clipboard command.
