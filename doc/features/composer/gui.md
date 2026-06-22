# Composer GUI

This is a normative supplemental GUI composition file for `design.md`. It owns composer feature slot mounts, layout relationships, and widget composition. Product behavior, submission, queues, history, image-label authority, quote insertion, and developer-instructions behavior remain in `design.md`.

## Composer Panel

Mount-into: main-window.user-input-panel

The composer panel is pinned above the status line inside the conversation column. The same panel layout is used for selected conversation threads and pending new-thread drafts.

The draft editor uses the external `text-input` widget contract adapted for multiline wrapping. The field wraps at visible width, avoids horizontal scrolling, and owns vertical scrolling only when wrapped content exceeds the panel height cap.

The panel grows and shrinks with wrapped draft line count up to its configured clamp and does not expose a manual resize handle. It does not include a persistent Run Turn or submit button.

Image markers render as compact inline atoms in the draft. Marker controls preserve the text-input editing contract for caret movement, selection, deletion, cut, paste, undo, and redo.

## Composer Marker Menu And Image Preview

Mount-into: main-window.overlays

Image marker activation opens an anchored `context-menu` with marker actions. The menu is bounded to the main OS window and does not submit the draft.

Image preview opens as a fitted popup over original image bytes. The preview is transient GUI chrome and does not open an external viewer or mutate draft, transcript, or backend state.
