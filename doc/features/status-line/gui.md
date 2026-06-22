# Status Line GUI

This is a normative supplemental GUI composition file for `design.md`. It owns status-line slot mounts, layout relationships, and widget composition. Product behavior, metadata authority, popup gating, compaction, stop semantics, and failure reporting remain in `design.md`.

## Status Line Strip

Mount-into: main-window.status-line

The status line is a fixed bottom strip anchored below the composer panel. It uses edge-to-edge separator treatment matching the main toolbar and is not transcript content.

The strip composes three left-to-right cells:

- Model and reasoning.
- Context space and rate-limit status.
- Turn state plus transcript-view turn position.

Cells use compact status typography and preserve fixed strip height while values change. Unknown and unavailable values render inside the existing cell geometry.

## Status Operation Popups

Mount-into: main-window.overlays

Model/reasoning, context operations, and turn operations open as bounded popups anchored to their status cells. They use `context-menu`, `segmented-status-bar`, `hold-to-confirm-button`, `tooltip`, and `disabled-command-tooltip` contracts where applicable.

Popups remain within the OS window bounds and close without changing transcript selection, draft content, or unrelated status cells.
