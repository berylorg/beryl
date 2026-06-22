# Notifications GUI

This is a normative supplemental GUI composition file for `design.md`. It owns notification feature slot mounts, layout relationships, and widget composition. Product behavior, notice queueing, sound eligibility, lifecycle notification policy, and failure handling remain in `design.md`.

## Main Workspace Notices

Mount-into: main-window.overlays

Notices mount in the main overlay layer near the top-right of the workspace window below the toolbar and thread strip.

The notice container renders at most one active notice at a time. It is bounded to the OS window and does not shift the toolbar, thread strip, transcript region, activity panel, composer panel, or status line.

Notice anatomy includes title, detail text, optional variant treatment, and a visible close action. Notice text supports ordinary text selection and copy.

Warning, error, and info variants resolve background, border, foreground, and close-control treatment from active theme notice roles.
