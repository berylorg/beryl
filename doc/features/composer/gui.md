# Composer GUI

This is a normative supplemental GUI composition file for `design.md`. It owns composer feature slot mounts and feature-specific widget configuration. Product behavior, submission, queues, history, image-label authority, quote insertion, persistence, and developer-instructions behavior remain in `design.md`; reusable panel, editor, marker, preview, and menu mechanics remain in their canonical widget specs.

## Composer Panel

Mount-into: main-window.user-input-panel

The composer feature mounts one project-local `conversation composer` in this slot. It is pinned below any branch-discussion status strip and above the global status line inside the conversation column and receives the selected Syndic thread's current durable draft.

The feature configures the composer's external `text-input` dependency as range-backed multiline,
editable, Enter-propagates, atom-clipboard-propagates, and rich-paste-propagates. It owns the
resulting submission, replacement-edit, clipboard-marker, and paste commands.

The feature supplies the composer height clamp as half the OS-window height, further constrained to preserve the transcript-region minimum height. The canonical widget grows and shrinks to wrapped content within that allocation and owns inner editor overflow.

CAS, root, transcript, or thread-catalog readiness configures the widget's submission-disabled state without making its editor inert. Beryl-home failure, a live admitted-resolution handoff gate, and archived-discussion state configure the inert state because those product states forbid draft mutation. Terminal handoff failure releases that gate and returns the unarchived discussion composer to the writable state.

Image references configure the editable-atom variant of the project-local `image marker`. The feature supplies final label text, marker identity, asset availability, and marker editing payloads; the `conversation composer`, `text-input`, and `image marker` own their reusable atom presentation and editing mechanics.

During selected-thread activation, the feature keeps the prior coherent composer and draft authoritative until it can publish the target draft with the target transcript. It configures the retained widget as activation-pending without merging early text into the unseen target draft.

When the backend-runtime-recovery feature mounts its `native lineage recovery prompt` into the same
slot, the ordinary composer is not visible or interactive. The composer feature retains its exact
bounded resident editor ranges, caret, compact selection, bounded undo frontier, inner scroll
position, and draft binding without
adding recovery controls to the canonical `conversation composer` widget. Successful recovery
restores that retained composer when no already-admitted turn owns the input.

## Composer Marker Menu And Image Preview

Mount-into: main-window.overlays

Image marker activation opens a built-in anchored `context menu` with `View` and `Remove` commands. The feature supplies their availability and effects. The menu is bounded to the main OS window and does not submit the draft.

`View` opens the project-local `image preview` over the owner-supplied original durable image resource. The feature supplies resource readiness, failure meaning, origin marker, and accessibility text. The canonical preview owns fitted popup layout and dismissal. On close it returns focus to the exact eligible marker; if that marker no longer exists, the feature focuses the active conversation-composer editor when editable or the active thread selector trigger when the composer is inert. Opening or closing the preview does not mutate draft, transcript, or backend state.
