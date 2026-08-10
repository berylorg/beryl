# Settings GUI

## Settings Window

Mount-into: settings-window.body

The full Settings window composes the externally registered `settings-window` with Beryl-supplied section and page models. Its left settings navigation selects the broad sections, and the selected right-pane page presents the feature-specific content as externally registered `settings-row` instances. Rows and their contained external controls provide the page's labels, descriptions, staged value controls, localized feedback, and page or row actions.

This is composition of the external settings-window and settings-row widgets; it introduces no project-local Settings shell or row widget.

## Diagnostics Page

The Diagnostics page is the selected right-pane page for the `Diagnostics` settings section. It contains an external `settings-row` for `Activity diagnostic capture`, configured as a choice field with `Disabled` and `Enabled` options. The row's external status-message area presents the bounded capture state and counters supplied by the Diagnostics feature.

This choice field and status presentation are a feature-local composition of `settings-window` and `settings-row`, not a new reusable widget: they add no reusable anatomy, interaction model, state family, layout algorithm, or UI roles beyond those canonical widgets.
