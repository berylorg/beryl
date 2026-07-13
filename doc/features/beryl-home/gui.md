# Beryl Home GUI

This is the normative supplemental GUI composition file for `design.md`. It owns the busy-home and unreadable-startup failure window compositions plus running-session store-failure notice configuration. Home ownership, timing, exit status, state-store failure behavior, and recovery rules remain in `design.md`.

## Busy Home Surface

Mount-into: busy-home-window.body

This is an explicitly feature-local arrangement rather than a project-local widget. It is one startup-only heading, explanatory text, countdown readout, and canonical `command button` stack with no reusable focus, selection, disclosure, scrolling, or state model beyond its children.

The surface is a compact centered vertical stack containing a heading that the Beryl home is already open, short explanatory text that another Beryl process owns it, the remaining automatic-exit time, and one text-labeled `Exit` command button.

The home path is not required as persistent body text. If shown for diagnosis, it is bounded, selectable, and truncates visually without becoming an alternate home picker.

The surface contains no Retry, Take Over, Choose Another Home, runtime, root, thread, Settings, or CAS controls. Closing the OS window has the same outcome as activating Exit.

## Home Failure Surface

Mount-into: home-failure-window.body

This is an explicitly feature-local arrangement rather than a project-local widget. It is one startup-only heading, bounded selectable detail, and canonical `command button` row; the non-resizable integration window supplies its fixed bound, so the arrangement introduces no reusable viewport or layout contract.

The surface is a compact centered vertical stack containing the heading `Beryl couldn't open its data`, bounded selectable failure detail, and a horizontal command row containing `Retry` and `Exit`.

Retry is the emphasized command while no retry is running. During an exact same-home retry it remains visible and disabled with progress conveyed through its label and accessibility state; Exit remains available.

The surface contains no Reset, Continue, Take Over, Choose Another Home, runtime, root, thread, Settings, or CAS controls. Repeated failure updates the same detail region without adding stacked notices or resizing the window.

## Running Store Failure Notice

Mount-into: main-window.overlays

Each existing main conversation window configures one project-local `main-window notice` in its persistent error variant while the shared Beryl-home store is failed or reopening. The notice identifies that Beryl cannot currently save or load application state, keeps bounded selectable detail, exposes no dismissal or manual home-selection command, and leaves the entire conversation shell visible beneath it.

Automatic same-home recovery updates the same stable notice rather than stacking retries. After validation succeeds, the persistent error notice is replaced by one ordinary dismissible informational recovery notice in each affected window.
