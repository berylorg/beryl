# Settings GUI

This is a normative supplemental GUI composition file for `design.md`. It owns settings feature slot mounts, layout relationships, and widget composition. Product behavior, staging, validation, persistence, dynamic tools, and feature-owned setting semantics remain in `design.md`.

## Settings Toolbar Control

Mount-into: main-window.toolbar

The Settings control is a toolbar `command button` in the trailing toolbar group after Exit. Activating it opens or reveals the dedicated settings window.

The control does not expose setting state inline in the main conversation toolbar. Feature-owned Apply, Revert, Save, Save As, Install Theme, and similar actions stay inside settings-window chrome, page headers, row value areas, or page action areas.

## Settings Window Configuration

The external `settings-window` widget directly supplies the top-level settings OS window. Beryl configures its left sidebar of broad sections and one right-pane page or subpage at a time; this configuration is not mounted inside a second Beryl-owned settings body.

This is a feature-local configuration of the external widget rather than a Beryl-specific settings-shell widget. The external `settings-window` owns reusable shell anatomy, focus, page navigation, scrolling, popups, and layout; Beryl supplies section identity and order, routed feature pages, labels, settings content, and commands without introducing another reusable shell contract.

The V1 sidebar sections are Themes, Operations, Notifications, and Agent. Sidebar rows do not expand into nested trees. Subpages open in the right pane with breadcrumb and back navigation while the sidebar remains at the broad section level.

The body stretches with the OS window. The sidebar has a bounded fixed logical width, and the main pane takes remaining width.

The settings shell is not an outer scrolling container. The sidebar and current page body own their own scrolling while page headers and action areas remain reachable.

The external footer remains visible with its standard `OK`, `Apply`, and `Cancel` controls. Beryl supplies their enabled state and the apply, accept-and-hide, and discard-and-hide commands defined by `design.md`; it does not replace or duplicate them inside a feature page.

Rows use the external `settings-row`, `text-input`, `color-input`, and `color-picker` widgets where applicable, plus ordinary command controls for row and page actions. Labels and descriptions wrap before controls shrink below useful widths.

The selected-page body exposes the `settings-window.page-content` integration slot. Feature-owned pages and subpages mount there and participate in the settings route without replacing the external shell or rendering as peer roots of the OS window.

## Ordinary Settings Pages

Mount-into: settings-window.page-content

The Settings feature contributes the Operations, Notifications, and Agent root pages. Only the page selected by the external settings-window route is visible and interactive. The Themes root page and Theme Editor subpage are separate contributions from the theming feature.

The Operations page configures external `settings-row` widgets for `Context compaction timeout` and `Draft autosave interval`. The Notifications page configures an `End-turn sound` row. The Agent page configures a multiline `Developer Instructions` row. Their values, validation messages, modified states, and enabled actions come from the owning feature models named by `design.md`.

Each page uses section headings followed by grouped external rows without adding a Beryl-owned row or card wrapper. The current page body owns vertical scrolling through the external settings-window contract; feature pages do not add a peer page scroll container.
