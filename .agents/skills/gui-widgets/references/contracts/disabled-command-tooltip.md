# Name

Canonical name: disabled-command-tooltip

# Purpose

Requires disabled command-capable controls to explain why activation is unavailable.

# References

Widgets:

- tooltip

# Applies To

Visible controls whose normal activation would invoke a command or command-like operation.

This includes command buttons, command rows, command rows in menus, action-menu readout segments, and other clickable command-capable controls.

This does not apply to passive labels, passive readouts, status text, decorative disabled-looking content, readonly fields, or non-command value controls.

# Rule

When an applicable control is visibly present but disabled, it must expose a tooltip explaining why activation is unavailable.

The tooltip text should name the closest actionable state or gate blocking activation, such as pending work, missing capability, invalid selection, backend unavailable state, stale projection, or incomplete metadata.

The disabled control must not invoke its command through pointer, keyboard, touch, or programmatic acceptance paths.

The tooltip obligation is behavioral. Tooltip visual styling, placement, dismissal, and text wrapping belong to the `tooltip` widget spec.
