# Backend Runtime Recovery GUI

This is a normative supplemental GUI composition file for `design.md`. It owns placement and widget composition for backend-unavailable recovery in main conversation windows. User-visible recovery behavior, command availability, preserved state, and exact backend binding remain in `design.md`.

## Backend-Unavailable Notice

Mount-into: main-window.overlays

Each affected main conversation window mounts one `main-window notice` configured with the error and persistent variants. The notice is bound to that window's selected thread and identifies the exact unavailable runtime in its owner-supplied title and bounded detail.

The owner-supplied command region contains a `command button` labeled `Retry`. The persistent variant omits the close command, so the notice remains visually anchored near the top-trailing edge while the conversation shell stays in place.

Retry progress and later notice revisions reuse the same stable notice identity. The composition does not add a modal backdrop, replace transcript content, reserve conversation-body layout space, or mount one process-global notice across multiple windows.
