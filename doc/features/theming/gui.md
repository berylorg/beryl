# Theming GUI

This is a normative supplemental GUI composition file for `design.md`. It owns theming feature slot mounts, layout relationships, theme editor composition, and theme-candidate widget composition. Product behavior, theme schema, repository authority, dynamic tools, validation, preview, install, and persistence remain in `design.md`.

## Themes Settings Page

Mount-into: settings-window.body

The Themes settings page is hosted inside the generic settings window. It appears as the Themes sidebar section and uses ordinary settings-window page layout.

Installed theme rows show theme name, stable id or copy-id action, active or modified state when applicable, and valid actions such as Activate, Rename, Delete, or Edit.

The active theme row exposes Save and Save As when the active theme has staged changes. Edit opens the theme editor subpage in the right pane.

## Theme Editor Subpage

Mount-into: settings-window.body

Theme editing opens as a settings subpage from the active theme row. The left settings sidebar remains on Themes, and the editor is not represented as a nested sidebar row.

The editor page header uses standard subpage breadcrumb text shaped as `Themes > <theme name>`. Save and Save As for modified active-theme drafts may appear in the page header as well as on the active theme row. Save and Save As are absent or disabled when there are no staged changes.

The editor body has two vertical regions:

- A bounded top theme role navigator.
- A lower selected-role property editor.

The role navigator presents the actual UI role schema tree as horizontally arranged columns. The first column contains the root role entry. Selecting a role opens the next column for that role's schema children.

Every navigator row is a real UI role id from the schema tree. Synthetic grouping labels, folder rows, or other non-role navigator items are invalid.

The role navigator owns horizontal scrolling when the role-column trail exceeds visible width. Each role navigator column owns vertical scrolling for role rows that exceed visible column height.

The property editor shows the selected role id and one row per hardcoded style property supported by that role. Unsupported role-property combinations are absent from the editor and do not appear through inheritance.

Property rows expose value-source selection, such as concrete value, static parent, ambient parent, or fallback. Concrete value controls appear only when the selected source requires one.

Role static parents are schema metadata displayed through the navigator rather than free-form editor fields. Property rows do not add per-row effective-value subtitles. Resolved samples may appear when useful, but samples are presentation-only and do not replace explicit property rows.

Dropdown source selectors use a down-facing thick triangle glyph visually matched to the step-in triangle family. Step-in navigation continues to use the right-facing thick triangle glyph.

Color-valued properties use the shared settings color input field and in-window color picker mechanics supplied through the settings-window widget family.

## Theme Candidate Code Panel Actions

Mount-into: main-window.transcript-region

Fenced transcript code blocks with language `beryl-theme` render through the shared project-local `code-panel` widget inside the transcript region.

A valid candidate panel may expose Beryl-owned Preview and Install Theme actions. While its candidate is the active transient preview, the originating code panel may expose Stop Preview.

Candidate validation feedback is bounded inside the code panel. Candidate actions do not create synthetic transcript rows or add unsaved candidates to the settings window.
