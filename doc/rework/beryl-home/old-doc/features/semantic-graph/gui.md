# Semantic Graph GUI

This is a normative supplemental GUI composition file for `design.md`. It owns semantic-graph slot mounts, layout relationships, and widget composition. Product behavior, graph invariants, graph persistence, tool contracts, provenance, and mutation validation remain in `design.md`.

## Graph Toolbar Toggle

Mount-into: main-window.toolbar

The Graph toggle is a toolbar `command-button` in the trailing toolbar group before Settings. It uses active button treatment while the graph overlay is visible.

Activating the toggle opens or closes the graph overlay without replacing the main workspace shell.

## Graph Overlay

Mount-into: main-window.overlays

The graph overlay floats above the conversation column. It anchors its horizontal bounds to the conversation column and its top edge below the thread strip.

The overlay has a fixed header strip and a graph browser viewport. It remains bounded within the visible conversation column, clamps height in small windows, and leaves toolbar, thread strip, composer, status line, and transcript layout in place.

The graph browser uses the project-local `column-browser` widget. It owns horizontal scrolling for the column trail, while each column owns vertical scrolling for its rows below a fixed column header.

Graph node context menus use the built-in `context-menu`, `anchored-context-menu`, `tooltip`, `disabled-command-tooltip`, and `hold-to-confirm-button` contracts. Menus and submenus are clamped within the OS window.

Feature-owned graph row or menu extensions compose through `semantic-graph.overlay-affordances`.

Summary tooltips are suppressed while a graph node context menu is open.
