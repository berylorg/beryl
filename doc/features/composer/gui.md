# Composer GUI

This is a normative supplemental GUI composition file for `design.md`. It owns composer feature slot mounts and feature-specific widget configuration. Product behavior, submission, queues, history, image-label authority, quote insertion, persistence, and developer-instructions behavior remain in `design.md`; reusable panel, editor, marker, preview, and menu mechanics remain in their canonical widget specs.

## Composer Panel

Mount-into: main-window.user-input-panel

The composer feature mounts one project-local `conversation composer` in this slot. It is pinned below any branch-discussion status strip and above the global status line inside the conversation column and receives the selected Syndic thread's current durable draft.

The feature configures the composer's external `text-input` dependency as range-backed multiline,
editable, Enter-propagates, atom-clipboard-propagates, and rich-paste-propagates. It owns the
resulting submission, replacement-edit, clipboard-marker, and paste commands.

The feature supplies the composer height clamp as half the OS-window height, further constrained to preserve the transcript-region minimum height. The canonical widget grows and shrinks to wrapped content within that allocation and owns inner editor overflow.

The mounted composition adds no manual resize handle and no persistent `Run Turn` or submit
command button. Submission remains an editor command under the product behavior in `design.md`.

CAS, root, transcript, or thread-catalog readiness configures the widget's submission-disabled state without making its editor inert. Beryl-home failure, a live admitted-resolution handoff gate, and archived-discussion state configure the inert state because those product states forbid draft mutation. Terminal handoff failure releases that gate and returns the unarchived discussion composer to the writable state.

Image references configure the editable-atom variant of the project-local `image marker`. The feature supplies final label text, marker identity, asset availability, and marker editing payloads; the `conversation composer`, `text-input`, and `image marker` own their reusable atom presentation and editing mechanics.

During selected-thread activation, the feature keeps the prior coherent composer and draft authoritative until it can publish the target draft with the target transcript. It configures the retained widget as activation-pending without merging early text into the unseen target draft.

When the backend-runtime-recovery feature mounts its `native lineage recovery prompt` into the same
slot, the composer host first fences new draft-changing input. Any active composition or pre-commit
edit settles through the ordinary exact edit boundary, and every admitted range-backed edit reaches
its exact host terminal, so the widget reaches the external contract's quiescent seed cut. The host
incorporates each terminal result into the current authoritative draft binding and revision, then
captures the external `text-input`'s exact
compact restoration seed: that binding, revision, and logical extent plus logical caret, selection,
host-owned undo-frontier, and semantic inner-scroll anchor facts. Only after that seed is complete does the slot
host unmount the ordinary `conversation composer` and publish the prompt.

Under the external `text-input` contract, unmount cancels cancellable page, segmentation,
clipboard, and geometry work, releases resident ranges and staged local capacity, and makes late
results obsolete. The composer feature retains only the compact host-owned restoration seed. It
retains no widget instance, resident source range or buffer, pending widget request or job, layout
cache, composition state, or staged widget mutation.

When successful recovery leaves no already-admitted turn owning the input, the composer feature
mounts a new range-backed editor against the draft's current authoritative binding and revision.
It validates the seed against that binding, revision, and logical extent and re-requests only the bounded source
ranges needed for the seeded caret, selection, viewport, and overscan before publishing the
coherent restored editor. Recovery controls remain outside the canonical `conversation composer`
widget.

## Composer Marker Menu And Image Preview

Mount-into: main-window.overlays

Image marker activation opens a bounded built-in `anchored context menu`, whose command-row
presentation follows the built-in `context menu`. It contains `View` followed by `Remove` and is
anchored to the activated project-local `image marker` inside the main OS-window overlay.

`View` composes the project-local `image preview` in the same overlay. The preview receives the
owner-supplied image presentation and origin marker. Its optional contextual-command anchor is the
attached-image contextual surface: activating that anchor opens a bounded built-in `anchored
context menu` containing `Copy` followed by `Save…` as command rows. The menu remains attached to
the preview anchor and separate from the preview's close command.

For Composer-origin image inspection, the feature supplies the stable surface fallback required by
the image-assets contract. If the exact origin marker disappears, or a picker or image-command flow
returns after the preview command anchor has disappeared, focus returns to the active conversation
composer's editor when that editor is visible and eligible to receive focus. Otherwise focus
returns to the active thread selector trigger in `main-window.toolbar` when that trigger remains
eligible for the inspection lifetime. The preview and its contextual command surface are
unavailable when neither fallback can remain eligible. This mapping applies after command success,
cancellation, decline, or failure without changing the generic command eligibility or focus-return
semantics owned by the image-assets feature.

Image-command eligibility, disabled reasons, effects, and focus behavior remain owned by
`doc/features/image-assets/design.md`. The canonical menu and preview widgets own their reusable
interaction mechanics. This is a feature-local arrangement because it only places and orders
existing canonical widgets and introduces no reusable control identity or interaction contract.
