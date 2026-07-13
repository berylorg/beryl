# Goals

Let users work across independent main conversation windows and reliably restore their task-oriented window layout after an intentional application exit or unexpected process termination.

Preserve each window's visible identity and placement without requiring auxiliary windows or transient GUI to become session state.

## Non-goals

- Allowing two main windows to open the same Syndic thread.
- Restoring Settings or other auxiliary and transient windows as session windows.
- Maintaining a redundant session snapshot outside Beryl's durable state store.
- Guaranteeing window restoration when the durable state store cannot be opened or read.

# Decisions

## GUI Supplement

- `gui.md` is the normative supplemental GUI composition file for the main-toolbar Exit command.

## Independent Main Windows

- Beryl may own multiple main conversation windows at once.
- Selection, navigation history, durable-draft editor projection, scroll state, flyout state, and visible loading or error presentation remain window-local. The draft record itself remains Syndic-owned.
- Shared runtime, catalog, storage-health, and thread-occupancy facts are presented independently by each affected window.
- `Ctrl+Shift+N` requests an additional main conversation window. Conversation-thread authority defines how that window acquires its own thread before becoming visible.
- When zero runtimes exist, `Ctrl+Shift+N` remains unavailable so runtime setup stays in the existing initial window.

## Ordinary Window Close

- Closing one main conversation window normally removes that window from the next restored session.
- Closing a window does not close or rearrange other main windows.
- When the closing window owns an active turn, the turn is interrupted before the window releases its thread claim.
- Closing the final main window through the ordinary window-close command durably records an empty restore set and then terminates Beryl normally.
- Final ordinary close is not the dedicated application Exit command: it does not preserve the closed window for restoration, and the next launch follows the empty-restore fallback acquisition below.

## Application Exit

- The main toolbar exposes a dedicated `Exit` command.
- Activating Exit marks the complete current set of open main conversation windows for restoration and then exits Beryl.
- The restore set captures each open main window's selected thread identity, position, size, and Windows virtual-desktop placement.
- Exit does not add auxiliary Settings windows or transient flyouts, menus, previews, or notices to the restore set.
- If durable recording of the restore set fails, Beryl does not claim the exit state was saved and uses the established per-window storage-failure alert rather than silently discarding the current layout.

## Unexpected Termination

- Panic, crash, OS reboot, and other external process termination preserve the last durable open-window set for the next start.
- Beryl does not intentionally clear the restore set merely because the process did not reach an orderly Exit command.
- Unreadable or corrupted durable state at startup is outside the window-restore guarantee.

## Restore Placement

- Restored main windows return to their previous positions and sizes when those placements remain valid.
- A window returns to its prior Windows virtual desktop when that desktop still exists.
- When its prior virtual desktop no longer exists, the window is placed deterministically on the first virtual desktop rather than the currently active desktop.
- Remaining windows whose prior virtual desktops still exist retain their own prior desktop and placement.
- Placement is best-effort when monitor topology, work areas, scale factors, or virtual-desktop configuration changed. Beryl keeps restored windows reachable rather than reproducing invalid off-screen geometry.

## Startup Surface

- Beryl reads only the minimal durable session state needed to determine the restore set before creating visible windows.
- After discovering that set, Beryl loads each restored window's selected thread identity and current draft before that window becomes visible so typing never races unseen persisted text.
- When the validated restore set is empty and at least one runtime exists, Beryl creates one replacement main window under the most recently used runtime and its most recently used root, falling back to that runtime's non-removable home root.
- Before that replacement window becomes visible, Beryl claims an eligible pristine empty thread in the chosen runtime/root or creates a new empty thread when none is reusable.
- When the restore set and runtime registry are both empty, Beryl creates the one permitted threadless initial shell for runtime onboarding.
- No special loading window is displayed during that minimal pre-window read.
- The first visible surface of every restored or replacement window is its ordinary main conversation shell.
- Catalog loading, selected-thread loading, and runtime warm-up continue progressively after the shell exists according to their owning feature contracts.
