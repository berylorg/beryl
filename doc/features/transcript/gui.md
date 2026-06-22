# Transcript GUI

This is a normative supplemental GUI composition file for `design.md`. It owns transcript feature slot mounts, layout relationships, and widget composition. Product behavior, narrative inclusion rules, provenance requirements, selection authority, and failure states remain in `design.md`.

## Transcript Region

Mount-into: main-window.transcript-region

The transcript region fills the stretchable conversation-column space between the thread strip and the lower conversation panels. It owns vertical transcript scrolling and does not render the shared visual scrollbar affordance.

Transcript rows use bounded presentation records from the transcript presentation system. Loading, missing-data, pending-range, stale-result, rejected-demand, and budget-fallback rows are presentation states rather than transcript content.

Large blocks may embed project-local `code-panel` widgets, media previews, attachment affordances, tables, or stable fallbacks. Nested scrollable widgets follow the project `scroll-ownership` contract so transcript scrolling and inner widget scrolling do not co-own the same pointer-wheel gesture.

## Transcript Menus And Previews

Mount-into: main-window.overlays

Transcript context menus are anchored to rendered content with stable geometry and provenance. They use the built-in `context-menu`, `anchored-context-menu`, `tooltip`, and `disabled-command-tooltip` contracts.

Preview popups for transcript-owned media or comparable resources are bounded to the OS window and close without replacing transcript content or changing selected-thread state.
