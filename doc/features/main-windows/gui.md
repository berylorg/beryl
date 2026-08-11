# Main Windows GUI

This is the normative supplemental GUI composition file for `design.md`. It owns only the New
Window and Exit command mounts, control composition and placement, and window-close or Exit-failure
notice contributions. Their workflow, availability, waiting, failure, notice-command behavior,
session, persistence, restore, and window-lifecycle behavior remain in `design.md`.

## New Window Command

Mount-into: main-window.toolbar

`New Window` is a text-labeled `command button` in the trailing toolbar group before Exit and
Settings. It exposes `Ctrl+Shift+N` as its accelerator and remains present when zero runtimes exist,
the 256-window capacity is full, or Application Exit owns the interaction gate. Each unavailable
state composes `disabled-command-tooltip` with the exact design-owned reason.

## Exit Command

Mount-into: main-window.toolbar

Exit is a text-labeled `command button` in the trailing toolbar group immediately before Settings. It remains visibly separate from the conversation-thread feature's [`two-segment split button`](../../gui/widgets/two-segment-split-button/spec.md) and thread selector so it cannot be mistaken for thread manipulation.

Exit invokes the dedicated application-exit workflow. It does not open a menu or flyout and does not expose window-restore state inline in the toolbar.

The command stays in the same toolbar position for its enabled, waiting, and disabled states. Its
disabled presentation composes `disabled-command-tooltip` with the design-supplied closest gate or
recovery explanation.

During an admitted barrier, the command uses the built-in `loading` state with the label exactly
`Exiting…`. That loading state is the waiting indication; the command is accessibly disabled, and
its tooltip is `Application Exit is waiting for active work and durable state.` Failure restores the
label `Exit` and its design-owned enabled or disabled state.

## Window-Close And Exit-Failure Notice Contributions

This feature mounts no `main-window notice`. When `design.md` supplies an ordinary window-close or
Exit-failure report for an affected window, the feature contributes one owner-configured record
with a stable notice identity, bounded title and detail, and an empty owner-command set to the
Notifications per-window arbiter.

For a qualifying backend-unavailable condition, the bounded detail points to the separately owned
persistent backend-unavailable notice; it does not duplicate that notice's `Retry` `command button`.
For a qualifying persistent Beryl-home store failure, the detail points to the separately owned
persistent home-failure notice and its automatic recovery, which has no manual command. When neither
persistent condition is eligible, the close or Exit-failure record remains commandless without a
substitute recovery control. Notifications owns admission, priority, persistence, replacement, and
the sole visible notice instance.
