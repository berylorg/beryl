# Threaded Decisions GUI

This is a normative supplemental GUI composition file for `design.md`. It owns threaded-decision GUI composition details. Product behavior, workflow state, branch creation, resolution, handoff, cleanup, authority, and failure recovery remain in `design.md`.

## Decision Parent Breadcrumb

Mount-into: main-window.toolbar

Decision child threads may show a compact clickable parent-thread breadcrumb in the toolbar when horizontal space allows.

The breadcrumb uses the parent thread display title, sizes to that label under normal toolbar space, and activates the exact bound parent thread. It remains visible from the selected child projection while another thread activation is pending.

## Decision Graph Affordances

Mount-into: semantic-graph.overlay-affordances

Threaded-decision controls appear inside the semantic graph overlay owned by `doc/features/semantic-graph/gui.md`.

Checklist-item graph rows can show active-branch, resolution, partial-resolution, and archive-failure indicators. The indicators use compact row affordances so ordinary graph navigation remains the primary row shape.

Graph node context menus can expose Start Decision Branch, Start Decision, retry, and resolution-related commands. Disabled commands remain visible with specific disabled reasons through the shared disabled-command tooltip contract.

The decision checklist item remains visible through the child thread title, tooltip, graph row, and visible bootstrap turn rather than as the primary toolbar breadcrumb.
