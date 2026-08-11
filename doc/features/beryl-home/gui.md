# Beryl Home GUI

This is the normative supplemental GUI composition file for `design.md`. It owns the busy-home and unreadable-startup failure window compositions plus running-session store-failure notice configuration. Home ownership, timing, exit status, state-store failure behavior, and recovery rules remain in `design.md`.

## Busy Home Surface

Mount-into: busy-home-window.body

This is an explicitly feature-local arrangement rather than a project-local widget. It is one startup-only heading, explanatory text, countdown readout, and canonical `command button` stack with no reusable focus, selection, disclosure, scrolling, or state model beyond its children.

The surface is a compact centered vertical stack containing a heading that the Beryl home is already open, short explanatory text that another Beryl process owns it, the remaining automatic-exit time, and one text-labeled `Exit` command button.

The home path is not required as persistent body text. If shown for diagnosis, it is bounded, selectable, and truncates visually without becoming an alternate home picker.

The surface contains no Retry, Take Over, Choose Another Home, runtime, root, thread, Settings, or CAS controls.

## Home Failure Surface

Mount-into: home-failure-window.body

This is an explicitly feature-local arrangement rather than a project-local widget. It is one startup-only heading, bounded selectable detail, and group of canonical `command button` controls; the non-resizable integration window supplies its fixed bound, so the arrangement introduces no reusable viewport or layout contract.

The surface is a compact centered vertical stack containing the heading `Beryl couldn't open its data`, bounded selectable failure detail, and a horizontal button group containing `Retry` and `Exit`.

`Retry` uses the primary `command button` variant. The feature supplies its current label, enabled state, progress accessibility state, and the ordinary `Exit` `command button` state from `design.md`.

The surface contains no Reset, Continue, Take Over, Choose Another Home, runtime, root, thread, Settings, or CAS controls. The bounded detail region retains one stable location without stacked notices or window resizing.

## Running Store Notice Contributions

This feature mounts no `main-window notice`. From the failure, reopening, and recovered states owned
by `design.md`, it supplies owner-configured records to the Notifications per-window arbiter.

The failed/reopening configuration uses one stable home-condition identity, the persistent error
variant, bounded selectable detail, and no close or manual home-selection command. The recovered
configuration uses a distinct dismissible informational record identity. Notifications owns their
priority, persistence, replacement, and sole visible notice composition, so these contributions add
no competing overlay or stacked panel.
