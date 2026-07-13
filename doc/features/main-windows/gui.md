# Main Windows GUI

This is the normative supplemental GUI composition file for `design.md`. It owns main-window lifecycle mounts and visible placement. Session membership, persistence, restore behavior, failure behavior, and window ownership remain in `design.md`.

## Exit Command

Mount-into: main-window.toolbar

Exit is a text-labeled `command button` in the trailing toolbar group immediately before Settings. It remains visibly separate from the New Thread split button and thread selector so it cannot be mistaken for thread manipulation.

Exit invokes the dedicated application-exit workflow. It does not open a menu or flyout and does not expose window-restore state inline in the toolbar.

When Exit is temporarily unavailable because the restore set cannot be recorded, it remains visible and disabled and explains the closest storage gate through `disabled-command-tooltip`.
