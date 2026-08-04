# Goals

Open one Beryl home as the durable application boundary, restore its main-window session without a visible loading screen, and fail safely when that home is already owned or its state store becomes unavailable.

Preserve the user's current window layout and last coherent work surfaces when durable state fails during a running session.

## Non-goals

- Allowing two Beryl processes to own one home concurrently.
- Sharing one Syndic store between different Beryl homes.
- Maintaining a redundant session-restore snapshot outside the Beryl home state store.
- Guaranteeing session restoration when the home state store is unreadable at startup.
- Choosing a runtime, root, or CAS process as part of opening the Beryl home.

# Decisions

## Supplemental Material

- `gui.md` is the normative supplemental GUI composition file for the busy-home surface, unreadable-startup failure surface, and running-session store-failure notice.
- Running-session storage failures use the existing main-window shell and notice placement rather than replacing each window with a new failure window.

## Home Opening

- A Beryl home is the physical directory containing the durable state owned by one Beryl installation context.
- Startup opens the configured Beryl home before it creates ordinary visible windows.
- Opening the home does not require a configured runtime, a root, CAS installation, CAS authentication, or CAS availability.
- When the home opens successfully, main-window restoration and the first visible shell follow `doc/features/main-windows/design.md`.
- A fresh home with zero runtimes opens the one initial threadless main conversation shell defined by `doc/features/conversation-threads/design.md`.

## Busy Home

- At most one OS process owns a Beryl home at a time.
- If another live process already owns the requested home, Beryl does not open the store, start CAS, restore main windows, or offer lock takeover.
- Beryl presents one compact dedicated busy-home startup window rather than an ordinary main conversation window.
- The busy-home window exposes only one explicit command, `Exit`.
- The busy-home process exits automatically after approximately five seconds when the user does not exit sooner.
- Busy-home exit uses process exit code `1`.
- A stale lock-file path by itself is not presented as a busy home; only failure to acquire the live ownership lock establishes the busy state.

## Unreadable Store At Startup

- If the Beryl-home state store cannot be opened or validated at startup, Beryl does not claim that prior windows or threads were restored.
- Beryl does not reconstruct the session from CAS, a second snapshot, cached catalog rows, or filesystem guesses.
- No ordinary main conversation window is created from unvalidated session records.
- The startup failure remains explicit and bounded; it must not silently create a fresh replacement home over the unreadable store.
- Beryl presents the home failure window with `Retry` and `Exit`.
- Retry targets only the same configured home, reruns open and validation, and creates ordinary restored windows only after success.
- A repeated failure keeps the home failure window and updates its bounded selectable detail. It never offers Reset, Choose Another Home, Take Over, or Continue Without History.
- Unlike the busy-home surface, unreadable startup does not auto-exit.

## Persistent Store Failure During A Session

- A persistent state-store failure never closes, removes, replaces, repositions, or changes the virtual-desktop placement of an existing main conversation window.
- Every existing window keeps its position, size, virtual-desktop placement, selected thread identity, and last coherent resident surface in memory.
- Already resident content may remain readable, selectable, and copyable when those actions require no new durable read or mutation.
- History loading and navigation, thread activation, draft editing, submission, settings, window-session mutation, runtime/root mutation, metadata mutation, and every other operation requiring Beryl-home state are unavailable while the failure persists.
- Beryl does not report a failed write as saved and does not fall back to CAS history or another authority.
- Once the failure is established as persistent, Beryl first closes further live-command
  authorization, then makes one best-effort attempt to interrupt each exact active conversation
  turn already known from the last coherent in-memory projection without closing its window, but
  only when retained dispatch evidence proves no earlier primary interruption may have crossed.
- This emergency path cannot claim durable stop admission after the store gate has failed. It never
  guesses a target, duplicates a possibly dispatched durable stop, retries an ambiguous
  interruption, releases a thread claim, or reports durable stop confirmation. One fixed
  process-local failure-generation guard permits at most one volatile request per exact target;
  same-home recovery starts from the last committed gate and lifecycle state.
- Turn output, lifecycle updates, or other incoming work that arrives while durable capture is unavailable is never presented as durably saved.
- Each affected main window presents the shared failure through its own disabled controls and established error notice.
- An operation that would close a window or exit while changing the durable restore set does not complete when that restore-set write fails; the current windows remain intact.
- If the process terminates before recovery, only the last successfully committed session and window state can survive; the preserved in-memory state is not a redundant restore snapshot.

## Recovery Boundary

- Recovery must validate the same configured home and state store before any gated operation resumes.
- A retry never substitutes a new home, clears the existing home, drops records, or selects another thread merely to make the shell interactive.
- If recovery cannot prove a healthy store, the application remains in the preserved fail-closed state.
- Running-session recovery automatically retries and validates the same configured home in the background for every affected window.
- After validation succeeds, Beryl reconciles every affected turn's exact state before it reenables work on that turn or publishes later turn state.
- Each affected main window uses the established error notice when failure begins and an informational recovery notice after the same home validates successfully. No separate recovery window replaces a running conversation shell.
