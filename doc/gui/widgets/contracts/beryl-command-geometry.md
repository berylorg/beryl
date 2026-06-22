# Name

Canonical name: beryl-command-geometry

# Purpose

Defines the shared Beryl geometry contract for command-capable controls that use button-like chrome.

# References

Widgets:

- command button

# Applies To

Beryl-owned command buttons, text buttons, icon-only buttons, square command controls, and command rows whose owning widget intentionally uses Beryl button geometry.

# Rule

Beryl-owned button-like command controls share one app-wide command-control height independent of primary, secondary, destructive, or disabled color roles.

Labels use the standard UI font family, shared button-label size, shared button-label line height, and active button role font weight.

Internal padding is centralized separately for vertical and horizontal axes. Normal text-labeled command buttons use the shared horizontal padding exactly and remain content-sized unless a finite-label contract reserves width for label changes.

Square or icon-only command controls may override horizontal padding or width only as needed to preserve a square footprint. Text buttons and icon-only buttons share the same outer height and corner shape.

Button containers preserve their own outer border and label padding under bounded-width truncation. Truncation may shorten label text but must not clip, mask, or hide the command control's right or bottom edge.

Controls whose visible text comes from a known finite cycling or toggle label set reserve width for the longest label in that set.

Geometry is invariant across normal, hover, pressed, active, focused, and disabled states. Interaction states must not change width, height, padding, border width, font size, line height, font weight, transform, shadow, or flex sizing.

Rounded corners for Beryl-owned button-like command controls and comparable rounded widgets come from one shared corner-shape value unless a concrete widget contract requires otherwise.
