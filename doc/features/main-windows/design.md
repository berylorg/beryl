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

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for the main-toolbar New Window and
  Exit commands.

## Independent Main Windows

- Beryl may own multiple main conversation windows at once, up to one stable configured capacity
  shared by ordinary creation and restoration. The capacity is 256 main windows per Beryl home and
  running process; it is independent of thread count, transcript size, or available monitor space.
- Selection, navigation history, durable-draft editor projection, scroll state, flyout state, and visible loading or error presentation remain window-local. The draft record itself remains Syndic-owned.
- Shared runtime, catalog, storage-health, and thread-occupancy facts are presented independently by each affected window.
- The visible `New Window` command and its `Ctrl+Shift+N` accelerator request an additional main
  conversation window. Conversation-thread authority defines how that window acquires its own
  thread before becoming visible.
- When zero runtimes exist, `New Window` remains visibly disabled so runtime setup stays in the
  existing initial window; its tooltip directs the user to runtime setup in New Thread.
- When every main-window slot is occupied, `New Window` remains visibly disabled, mutates no claim or
  session state, and explains that the process window capacity is in use through the ordinary
  disabled-command treatment.

## Ordinary Window Close

- These ordinary close and Exit rules do not govern
  [fatal crash reporting](../crash-reporting/design.md). A panic terminates the application without
  another session write; its independent report has no main-window restore identity.

- Closing one main conversation window normally removes that window from the next restored session.
- Closing a window does not close or rearrange other main windows.
- Activating close shows one close-in-progress state and keeps the window visible until its dirty
  draft and session update have settled. New mutations in that window are unavailable during this
  state, while its last coherent content remains readable, selectable, scrollable, and copyable.
- Closing a nonfinal main window flushes its current draft and session, then detaches its view and
  GUI claim. It never stops, joins, pauses, or otherwise changes an active turn,
  context compaction, accepted queued input, terminal-history convergence, or
  scheduled automatic continuation for that exact thread.
- If a draft or session obligation fails or cannot be proven complete, the window returns to its
  coherent open state with its thread claim, resident editor, and last presentation intact. It
  reports a bounded commandless close-failure notice. It does not stop the background execution.
- For a dirty-draft or session failure that also establishes persistent Beryl-home store failure,
  the commandless close-failure notice directs the user to the separately owned persistent home-
  failure notice. The [Beryl Home feature](../beryl-home/design.md) owns automatic same-home
  recovery and exposes no manual command on that running-session notice. A dirty-draft or session
  failure that does not make that persistent notice eligible likewise exposes no recovery command.
- Beryl neither guesses completion nor retries the close automatically. After the blocking state is
  coherent again, another close attempt requires a new ordinary window-close activation.
- After every required obligation succeeds, Beryl removes the window from the restore set, releases
  its thread claim, and closes that window.
- Closing the final main window never leaves Beryl running in a tray or without a main window. If
  no process work remains, it durably records an empty restore set and terminates Beryl normally.
- When final ordinary close finds any active turn, context compaction, admitted pending work,
  in-flight request handling, scheduled continuation, or terminal-history convergence, Beryl
  presents one platform-native confirmation
  dialog. `Cancel`, `Escape`, and OS dialog dismissal leave every window and work item unchanged;
  its positive command is `Exit Beryl`.
- Confirming `Exit Beryl` begins the same all-work shutdown as the dedicated Exit command. It freezes
  new dispatch, cancels scheduled automatic continuations, preserves already accepted queued input,
  requests or joins each exact interruptible operation's sole soft stop, and waits for exact active work
  and terminal history to settle before
  process exit. It records an empty restore set after the shutdown succeeds.
- The confirmation covers the complete work set captured by atomic shutdown-barrier admission. Later
  revisions or successor states of that same work remain inside it and never prompt again. If new work
  appears while the no-work final-close fast path is being admitted, Beryl asks before effects begin.
- Concurrent final-close requests serialize behind one close owner and share its confirmation or
  shutdown state. Final-window designation is revalidated under that serialization so Beryl never
  exits with zero resident windows by race. If confirmation is cancelled or any shutdown, flush, or durable-session obligation
  fails or remains unproven, the final window remains open with its last coherent presentation.
- Final ordinary close is not the dedicated application Exit command: it records an empty restore
  set, while Exit preserves the current layout for restoration.

## Application Exit

- The main toolbar exposes a dedicated `Exit` command.
- Activating Exit first uses the same native confirmation when any process work remains, including
  unviewed work. Cancel leaves all work and windows unchanged. Confirmed activation, or a revalidated
  no-work activation, begins one application-wide barrier; all windows remain until it succeeds.
- The barrier covers work with and without an open view: active turns, context compactions, admitted
  pending work, in-flight request handling, scheduled continuation, and terminal-history
  convergence. For every interruptible exact
  operation it requests or joins the sole exact soft-stop path at most once and otherwise waits for
  exact terminal or authority-loss completion. An operation already stopping is joined; Exit never
  treats compaction as coarse thread activity or exposes or issues a hard stop, coarse stop,
  escalation, force exit, or second interruption.
- Exit freezes new dispatch and cancels every scheduled automatic lifecycle continuation before it
  becomes another turn. A continuation that already became a turn remains ordinary thread work and
  must settle as part of the same barrier; already accepted queued input remains preserved.
- Exit waits for every open window's dirty-draft flush, all exact work's terminal-history outcome,
  and admitted draft, session, and restore-set obligations to settle durably. An indeterminate
  durable outcome remains part of the barrier until same-home reconciliation proves its result.
- While the barrier is active, every visible Exit command is disabled with a waiting indication.
  Repeated activation cannot create another exit attempt or another interruption.
- Barrier admission freezes the complete current mutation set. New Window, New Thread, thread or
  runtime switching, draft edits, paste, submission, steering, Settings mutation, theme mutation,
  and ordinary window close become visibly unavailable in every window with the reason
  `Application Exit is waiting for active work and durable state.` Read-only selection, scrolling,
  copying, and inspection remain available. No accepted mutation or newly created window may enter
  outside the captured barrier.
- The Exit control keeps its stable toolbar position, changes its label to `Exiting…`, shows the
  `command button` loading state, visibly becomes disabled, and uses the same exact
  waiting reason in its disabled tooltip.
- After the complete restore set, orderly-exit intent, and all-work terminal history are durable,
  Beryl closes all application windows and terminates the process.
- The restore set captures each open main window's selected thread identity, position, size, and Windows virtual-desktop placement.
- Exit does not add auxiliary Settings windows or transient flyouts, menus, previews, or notices to the restore set.
- Before a barrier begins, any feature-owned gate that cannot yet settle safely keeps Exit visible
  but disabled and explains the closest blocking state; activation starts no queued or deferred
  exit. In particular, an in-progress Settings reconciliation must finish and require a new Exit
  activation after the command becomes available again.
- When restore-set storage is already unavailable, Exit remains visible but disabled and its
  explanation points to the persistent Beryl-home failure notice and its automatic same-home
  recovery; that notice has no manual command, and Exit activation starts no barrier.
- If an enabled Exit activation later fails or cannot prove its barrier complete, Beryl does not
  exit or close a subset of windows. Every window, thread claim, resident editor, and last coherent
  presentation remains intact. The affected windows report the blocking turn, draft, session, or
  storage failure through commandless Exit-failure notices.
- A failure belonging to unviewed work is reported through a bounded commandless Exit-failure
  notice in the invoking surviving window. Reporting never opens or selects that thread; when
  no matching persistent backend condition is eligible it offers no substitute Retry command.
- A blocking turn or active-work failure that also establishes selected-thread runtime/backend
  unavailability in an affected window directs the user to that window's separately owned
  persistent backend-unavailable notice;
  only that notice exposes the backend-runtime recovery feature's exact `Retry` states. Otherwise
  the Exit-failure notice exposes no recovery command.
- A draft, session, restore-set, or storage failure that also establishes persistent Beryl-home
  store failure directs the user to the separately owned persistent home-failure notice and its
  automatic same-home recovery. Neither the Exit-failure notice nor that persistent home notice
  exposes a manual recovery command. Other draft, session, restore-set, or storage failure notices
  are also commandless.
- The application-wide interaction gate is removed atomically from that coherent state. Every
  previously eligible mutation surface is re-enabled, and the toolbar label returns to `Exit`; a
  new attempt requires explicit activation
  after the blocking state is coherent again.

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

- Startup presents the complete valid restore set or presents the established startup-failure
  surface. It never exposes an arbitrary prefix, subset, overflow window, or substitute thread when
  any required restored-window state is missing, invalid, duplicated, or unreadable.
- Every restored or replacement window becomes visible only when its selected thread and durable
  draft form one coherent first-presentable editor state. Its first composer can display the
  current visible content and accept input without racing unseen persisted text.
- When the validated restore set is empty and at least one runtime exists, Beryl creates one replacement main window under the most recently used runtime and its most recently used root, falling back to that runtime's non-removable home root.
- Before that replacement window becomes visible, Beryl claims an eligible pristine empty thread in the chosen runtime/root or creates a new empty thread when none is reusable.
- When the restore set and runtime registry are both empty, Beryl creates the one permitted threadless initial shell for runtime onboarding.
- No special loading window is displayed during bootstrap.
- The first visible surface of every restored or replacement window is its ordinary main conversation shell.
- Catalog loading, selected-transcript loading, and runtime warm-up continue progressively after the shell exists according to their owning feature contracts.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers: none
