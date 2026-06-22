# Settings GUI

This is a normative supplemental GUI composition file for `design.md`. It owns settings feature slot mounts, layout relationships, and widget composition. Product behavior, staging, validation, persistence, dynamic tools, and feature-owned setting semantics remain in `design.md`.

## Settings Toolbar Control

Mount-into: main-window.toolbar

The Settings control is a toolbar `command-button` in the trailing toolbar group after flexible space. Activating it opens or reveals the dedicated settings window.

The control does not expose setting state inline in the main workspace toolbar. Feature-owned Apply, Revert, Save, Save As, Install Theme, and similar actions stay inside settings-window chrome, page headers, row value areas, or page action areas.

## Settings Window Body

Mount-into: settings-window.body

The settings window body uses the external `settings-window` widget family. It contains a left sidebar of broad sections and one right-pane page or subpage at a time.

The V1 sidebar sections are Themes, Operations, Notifications, Agent, and Graph. Sidebar rows do not expand into nested trees. Subpages open in the right pane with breadcrumb and back navigation while the sidebar remains at the broad section level.

The body stretches with the OS window. The sidebar has a bounded fixed logical width, and the main pane takes remaining width.

The settings shell is not an outer scrolling container. The sidebar and current page body own their own scrolling while page headers and action areas remain reachable.

Rows use the external `settings-row`, `text-input`, `color-input`, and `color-picker` widgets where applicable, plus ordinary command controls for row and page actions. Labels and descriptions wrap before controls shrink below useful widths.
