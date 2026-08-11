# Theming GUI

This is a normative supplemental GUI composition file for the [feature design](design.md). It owns
theming feature slot mounts, external `settings-row` configuration, `theme editor` configuration,
and theme-candidate `code panel` composition. Product behavior, visible validation and repository outcomes, dynamic
tools, preview, install, and persistence remain in `design.md`; schema, repository, publication,
and arbitration architecture remain in the [theme runtime system](../../systems/theme-runtime/design.md).

## Themes Settings Page

Mount-into: settings-window.page-content

The Themes settings page is hosted inside the external `settings-window`. It appears as the Themes
sidebar section and uses the page-local split-list variant for installed themes plus ordinary
external `settings-row` widgets for the selected theme's bounded detail and actions.

The registered external `settings-window` uses its revision-bound paged page-local split-list
variant for the feature's logical installed-theme count, stable theme identities, and bounded
visible pages. Its specification owns fixed-height windowing and bounded overscan and preserves
stable selection, focus, popup anchoring, and scroll position across coherent same-page refreshes.
Beryl does not add a second settings shell.

Each installed-theme split item supplies only its stable item identity, theme name label, optional
stable-id subtext, applicable durable-active, Settings-staged, or document-modified preview, and
selection state. It supplies no row action or embedded command control.

The selected theme's bounded detail uses external `settings-row` widgets. Its stable-id row exposes
the design-owned `Copy ID` row action. The finite page-action area holds the design-owned `Activate`,
`Rename`, `Delete`, and `Edit` commands as canonical `command button` widgets. For a referenced
selected theme, Delete remains visibly disabled and uses the design-owned
reference explanation. Activate contributes the selected identity to the Settings window-wide
draft; it does not add a page-local Apply control. None of these commands is mounted inside a
split-list item.

The active-theme scalar is presented through an external `settings-row`. When that Settings-owned
row is modified and Reset is valid, its standard row context-action placement exposes `Reset`.
This is the visible Reset placement for the active-theme scalar; it is not a theme-editor command,
page-header action, or new theming-specific control. Its exact scalar-only effect is defined by the
feature design.

Save and Save As use the selected active theme's action-only detail row when the feature design
exposes them. They remain visually distinct from the external Settings footer and never substitute
for Apply or OK. Edit opens the theme editor subpage in the right pane.

Refresh and activation feedback may mark the affected split item through its preview. When that item
is selected, its external detail row contains the bounded message and its Retry `command button` as
a row action. When no item exists for the saved active identity, the page-level active-theme area
contains the feedback and Retry command.

## Theme Editor Subpage

Mount-into: settings-window.page-content

Theme editing opens as a settings subpage from the selected active theme's Edit page action. The
left settings sidebar remains on Themes, and the editor is not represented as a nested sidebar row.

The editor page header uses standard subpage breadcrumb text shaped as `Themes > <theme name>`.
When the feature design exposes Save and Save As, the same commands also occupy the page-header action
area while retaining their selected-theme action-only detail-row placement.

The page body contains the project-local `theme editor` widget. The page may also contribute
external `settings-row` composition for feature-owned theme-document inputs, including the Save As
name. These inputs have no setting id, do not join the window-wide Settings draft, and do not affect
the external footer's modified, Apply, or OK state. The external settings window continues to own
the page header, page scroll, breadcrumb navigation, page actions, settings-row field mechanics,
and transient in-window popups.

The theming feature supplies the widget with the theme-runtime projection, selected role, stable
role and property ids, resolved presentation samples, supported property rows, feature-owned
theme-document staged values, and localized validation state. It supplies only real UI role ids;
synthetic grouping rows are invalid.

For the selected role, the feature supplies one external `settings-row` per supported property.
These rows use the theming feature's document-draft modified and validation presentation, not the
Settings feature's staged-value ownership. Unsupported role-property combinations are absent. Rows
expose the allowed value-source choices, such as concrete value, static parent, ambient parent, or
fallback, and expose a concrete value control only when the selected source requires one.
Value-source choice controls use the external settings choice-control family's down-facing thick
triangle, visually paired with the theme editor's right-facing child-navigation affordance.

Static parents remain schema metadata rather than free-form editor fields. Property rows do not add per-row effective-value subtitles. Resolved samples are presentation-only and do not replace explicit property rows. Color-valued properties use the external settings color-input and color-picker path reached through `settings-row`.

The project-local [theme editor widget specification](../../gui/widgets/theme-editor/spec.md) owns
navigator anatomy, selection and focus behavior, nested
scrolling, bounded role-row realization, layout, variants, diagnostics, and UI roles. This feature
composition supplies only theming-specific content and commands.

## Theme Candidate Code Panel Actions

Mount-into: transcript.code-panel-actions

Fenced transcript code blocks with language `beryl-theme` render through the shared project-local `code panel` widget inside the transcript region.

A candidate panel places the design-supplied Preview, Install Theme, and Stop Preview commands among the `code panel`'s optional header controls.

## Theme Candidate Code Panel Feedback

Mount-into: transcript.code-panel-feedback

For the exact originating `beryl-theme` code panel, the feature supplies the bounded panel-local
validation or application feedback defined by `design.md`. The shared `code panel` owns feedback
placement and bounds. This contribution does not create a synthetic transcript row, a competing
notice, or another code-panel surface.
