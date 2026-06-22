# Name

Canonical name: expected-action-availability

# Purpose

Defines how expected command-capable controls remain visible when temporarily unavailable.

# References

Contracts:

- disabled-command-tooltip

# Applies To

Action buttons, command buttons, command rows, menu command rows, direct-action readout segments, and other command-capable controls that are normally part of the current object's UI.

# Rule

When an action is normally part of the UI for the current object or context, it remains visible while temporarily unavailable.

Temporarily unavailable actions render disabled rather than disappearing, and they expose a tooltip or equivalent local affordance explaining the specific unavailable reason.

The unavailable reason should name the closest actionable Beryl state or gate, such as a pending operation, missing capability, stale projection, incomplete metadata, invalid current selection, or backend-unavailable state.

An action may be absent when it is not part of the current object's UI, when the user is not in the action's context, or when the owning feature intentionally uses progressive disclosure for actions the user would not reasonably expect there.

Disabled command-capable controls must not execute through pointer, keyboard, touch, menu acceptance, or programmatic acceptance paths.
