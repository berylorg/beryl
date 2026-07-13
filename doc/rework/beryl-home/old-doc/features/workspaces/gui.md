# Workspaces GUI

This is a normative supplemental GUI composition file for `design.md`. It owns workspace feature slot mounts, layout relationships, and widget composition. Product behavior, persistence, validation, disabled states, and recovery remain in `design.md`.

## Workspace Toolbar Control

Mount-into: main-window.toolbar

The Workspaces control is a normal text-labeled `command-button` in the leading toolbar group. It uses the shared Beryl command geometry contract and sizes to its label instead of reserving a fixed chrome column.

Activating the control opens the workspace picker popup. The toolbar control remains visible while the picker is open and does not become a persistent workspace-name label separate from the control.

## Workspace Picker Popup

Mount-into: main-window.overlays

The workspace picker is an anchored popup opened from the Workspaces toolbar control. It remains within the main OS window bounds and closes without replacing the main workspace shell.

The popup contains two side-by-side columns separated by a vertical divider:

- A `Workspaces` column with a filter field above a divided list.
- A `Members` column with a fixed runtime selector row above a divided member list.

The `Workspaces` column uses a `single-line-text-field` for filtering, compact rows for workspaces, row-edge `context-menu` actions, and `hold-to-confirm-button` behavior for destructive delete confirmation.

The `Members` column uses selector controls for runtime choice, command rows for attachment, compact member rows with primary and secondary text, row-edge `context-menu` actions, and confirmation affordances for detach.

Rows use the shared left-edge accent marker for current workspace and primary member indication. Long workspace names, member labels, and member paths wrap inside the popup rather than forcing outer window scrolling.
