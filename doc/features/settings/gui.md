# Settings GUI

This is a normative supplemental GUI composition file for `design.md`. It owns settings feature slot mounts, layout relationships, and widget composition. Product behavior, drafts, validation, persistence, and feature-owned setting semantics remain in `design.md` and the named owning feature documents.

## Settings Toolbar Control

Mount-into: main-window.toolbar

The Settings control is a toolbar `command button` in the trailing toolbar group after Exit. Activating it opens or reveals the dedicated settings window.

The control does not expose setting state inline in the main conversation toolbar. Settings-owned
`Apply` and row-level `Reset`, plus owning-feature actions such as `Save`, `Save As`, and
`Install Theme`, stay inside settings-window chrome, page headers, row value areas, or page action
areas.

## Settings Window Configuration

The external `settings-window` widget directly supplies the preheated top-level settings OS window
that remains hidden when inactive. Beryl configures its left sidebar of broad sections and one
right-pane page or subpage at a time; this configuration is not mounted inside a second Beryl-owned
settings body.

This is a feature-local configuration of the external widget rather than a Beryl-specific settings-shell widget. The external `settings-window` owns reusable shell anatomy, focus, page navigation, scrolling, transient in-window popups, and layout. The Settings feature supplies section identity and order, routed feature pages, window-wide draft and modified presentation, row availability and validation presentation, and global footer action state. Owning features supply their row meaning, domain-validation results, defaults, and feature-specific actions. This composition introduces no second reusable shell contract.

The V1 sidebar sections are Themes, Operations, Notifications, and Agent. Sidebar rows do not expand into nested trees. Subpages open in the right pane with breadcrumb and back navigation while the sidebar remains at the broad section level.

The body stretches with the OS window. The sidebar has a bounded fixed logical width, and the main pane takes remaining width.

The settings shell is not an outer scrolling container. The sidebar and current page body own their own scrolling while page headers and action areas remain reachable.

The external footer remains visible with its standard `OK`, `Apply`, and `Cancel` controls. The
Settings feature supplies their enabled, disabled, and reconciling presentation plus the global
commands defined by `design.md`; it does not replace or duplicate them inside a feature page.

Rows use the external `settings-row`, `text-input`, `color-input`, and `color-picker` widgets where
applicable, plus canonical `command button` widgets for row and page actions. Their reusable field
and row layout remains owned by the registered external widget specs.

The Settings feature maps row-local validation and status feedback to the external `settings-row`
message part. Page-level or window-wide feedback with no exact row anchor maps to one feature-local
inline-message region in the selected-page body, with any applicable recovery commands composed as
canonical `command button` widgets. This region is a feature-local arrangement of bounded
owner-supplied message content and existing controls; it introduces no reusable control identity,
state model, or interaction contract.

The selected-page body exposes the `settings-window.page-content` integration slot. Feature-owned pages and subpages mount there and participate in the settings route without replacing the external shell or rendering as peer roots of the OS window.

## Ordinary Settings Pages

Mount-into: settings-window.page-content

The Settings feature contributes the Operations, Notifications, and Agent root pages. Only the page selected by the external settings-window route is visible and interactive. The Themes root page and Theme Editor subpage are separate contributions from the theming feature.

The Operations page configures external `settings-row` widgets for `Context compaction timeout` and `Draft autosave interval`. The Notifications page configures an `End-turn sound` row. The Agent page configures a multiline `Developer Instructions` row. The Settings feature supplies their staged values, modified and generic commit states, and window-wide actions; each owning feature supplies domain validation, active/default values, semantics, and feature-specific actions as assigned by `design.md`.

Each page uses section headings followed by grouped external rows without adding a Beryl-owned row or card wrapper. The current page body owns vertical scrolling through the external settings-window contract; feature pages do not add a peer page scroll container.
