# Theme Editor

This is a normative supplemental design file for `doc/features/theming/design.md`. It owns the theme editor UI and interaction contract. The theming feature entry point owns the theme model, repository, candidate workflow, and tool boundary.

## Entry And Navigation

- Theme editing opens a settings subpage from the active theme row on the Themes settings page.
- The editor is hosted inside the generic settings window defined by `doc/features/settings/design.md`.
- The left settings sidebar remains on `Themes`; the editor is not represented as a nested sidebar row.
- The editor page header uses standard subpage breadcrumb text shaped as `Themes > <theme name>`.
- Save and Save As actions for modified active-theme drafts may appear in the theme editor page header as well as on the active theme row.
- Save and Save As are absent or disabled when there are no staged changes.
- The default and minimum settings-window sizes must keep page-header Save As reachable without horizontal clipping.

## Layout

- The editor body has two vertical regions: a bounded top theme role navigator and a lower selected-role property editor.
- This layout is local to the editor content area and must not become a second persistent settings navigation column.
- The editor uses settings-window scrolling rules: the page body owns vertical scrolling while the page header and active action area remain reachable.
- The theme role navigator owns horizontal scrolling when its role-column trail exceeds visible width.
- Each role navigator column owns normal vertical scrolling for role rows that exceed the visible column height.
- The selected-role property editor owns normal vertical scrolling for property rows that exceed its visible height.

## Role Navigator

- The theme role navigator presents the actual UI role schema tree as horizontally arranged columns.
- The first column contains the root role entry.
- Selecting a role opens the next column for that role's schema children.
- Every navigator row is a real UI role id from the schema tree.
- Synthetic grouping labels, folder rows, or other non-role navigator items are invalid and must be treated as a design violation.
- Navigator selection is stored and reconciled by role id rather than row index.
- Selecting a role changes only the property editor for the current page.

## Property Editor

- The selected-role property editor shows the selected role id and one row per hardcoded style property supported by that role.
- Unsupported role-property combinations are absent from the editor and do not appear through inheritance.
- Role static parents are schema metadata displayed through the navigator rather than free-form editor fields.
- Property rows expose value-source selection, such as concrete value, static parent, ambient parent, or fallback.
- Property rows expose the concrete value control only when the selected source requires one.
- Property rows do not add per-row effective-value subtitles.
- Resolved samples may appear when useful, but samples are presentation-only and do not replace explicit property rows.

## Controls

- Dropdown source selectors use a down-facing thick triangle glyph visually matched to the step-in triangle family.
- Step-in navigation continues to use the right-facing thick triangle glyph.
- Color-valued properties use the shared settings color input field and in-window color picker mechanics.
- The settings-window color picker currently remains part of `gpui-settings-window`; a later extraction into a separate color-picker crate must preserve the same Beryl-facing color input behavior.

## Validation And Failure

- Unsupported role-property combinations, malformed concrete values, stale theme ids, stale role ids, repository failures, and unsafe draft conflicts render bounded editor-local or settings-page feedback.
- Validation failures must not partially mutate active theme state, installed themes, settings drafts, transcript content, GPUI widgets, or repository files.
- Closing or hiding the settings window without applying or saving discards unapplied staged editor changes according to the generic settings-window draft contract.
