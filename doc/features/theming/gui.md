# Theming GUI

This is a normative supplemental GUI composition file for `design.md`. It owns theming feature slot mounts, external settings-row configuration, theme-editor configuration, and theme-candidate code-panel composition. Product behavior, theme schema, repository authority, dynamic tools, validation, preview, install, and persistence remain in `design.md`.

## Themes Settings Page

Mount-into: settings-window.page-content

The Themes settings page is hosted inside the external `settings-window`. It appears as the Themes sidebar section and uses ordinary settings-window page layout and external `settings-row` widgets.

Installed theme rows show theme name, stable id or copy-id action, active or modified state when applicable, and valid actions such as Activate, Rename, Delete, or Edit.

The active theme row exposes Save and Save As when the active theme has staged changes. Edit opens the theme editor subpage in the right pane.

## Theme Editor Subpage

Mount-into: settings-window.page-content

Theme editing opens as a settings subpage from the active theme row. The left settings sidebar remains on Themes, and the editor is not represented as a nested sidebar row.

The editor page header uses standard subpage breadcrumb text shaped as `Themes > <theme name>`. Save and Save As for modified active-theme drafts may appear in the page header as well as on the active theme row. Save and Save As are absent or disabled when there are no staged changes.

The page body contains the project-local `theme editor` widget. The page may also contribute non-editor external settings rows, including the Save As name row, through the external page composition. The external settings window continues to own the page header, page scroll, breadcrumb navigation, page actions, settings-row field mechanics, and transient popups.

The theming feature supplies the widget with the actual hardcoded UI role schema projection, selected role, stable role and property ids, resolved presentation samples, supported property rows, staged values, and localized validation state. It supplies only real UI role ids; synthetic grouping rows are invalid.

For the selected role, the feature supplies one external `settings-row` per hardcoded supported property. Unsupported role-property combinations are absent. Rows expose the allowed value-source choices, such as concrete value, static parent, ambient parent, or fallback, and expose a concrete value control only when the selected source requires one.

Property source-choice controls use a down-facing thick triangle visually matched to the theme editor's right-facing child-navigation affordance.

Static parents remain schema metadata rather than free-form editor fields. Property rows do not add per-row effective-value subtitles. Resolved samples are presentation-only and do not replace explicit property rows. Color-valued properties use the external settings color-input and color-picker path reached through `settings-row`.

The retained navigator anatomy, selection/focus behavior, nested scrolling, bounded role-row realization, layout, variants, diagnostics, and UI roles are owned only by `doc/gui/widgets/theme-editor/spec.md`. This composition does not redesign the theme hierarchy, inheritance model, editor navigation, or editor workflow.

## Theme Candidate Code Panel Actions

Mount-into: transcript.code-panel-actions

Fenced transcript code blocks with language `beryl-theme` render through the shared project-local `code-panel` widget inside the transcript region.

A valid candidate panel may expose Beryl-owned Preview and Install Theme actions. While its candidate is the active transient preview, the originating code panel may expose Stop Preview.

Candidate validation feedback is bounded inside the code panel. Candidate actions do not create synthetic transcript rows or add unsaved candidates to the settings window.
