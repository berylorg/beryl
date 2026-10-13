# Passive Pointer Focus

Omitting a command handler does not make a GPUI surface passive with respect to focus. The pinned
GPUI `div` implementation installs a bubbling mouse-down focus action for every `track_focus`
element unless the event default is prevented.

Native transcript-menu qualification exposed a disabled row whose click reached the underlying
transcript focus handler. Edit remained unavailable, but subsequent Escape no longer reached the
popup. Wider model-menu qualification also exposed passive model readouts transferring focus to
a tracked ancestor. These are distinct effects of leaving pointer focus ownership implicit.

The canonical anchored menu now retains popup focus and stops inside left-click propagation.
Model segments prevent the default mouse-focus action regardless of command availability, while
command activation remains enabled-only. Acceptance evidence belongs in the
[transcript mounting audit](../audits/transcript-mounting-readiness.md).

Native fixtures must use current painted target geometry and an eligible focus owner. A window
handle lookup alone does not establish native activation, and range-input `focus` does nothing
before the widget is mounted and enabled. Pump frames through `App::update_window` outside an
entity update; drawing while the root entity is borrowed re-enters its render and panics. Keep
the same focus, disabled-activation and duplicate-request assertions when correcting that setup.
