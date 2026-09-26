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

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for the busy-home surface, unreadable-startup failure surface, and running-session store-failure notice.
- Running-session storage failures use the existing main-window shell and notice placement rather than replacing each window with a new failure window.

## Home Opening

- The configured Beryl home is the durable boundary for one Beryl installation context.
- No ordinary window appears until the configured Beryl home opens successfully.
- Opening the home does not require a configured runtime, a root, CAS installation, CAS authentication, or CAS availability.
- When the home opens successfully, main-window restoration and the first visible shell follow `doc/features/main-windows/design.md`.
- A fresh home with zero runtimes opens the one initial threadless main conversation shell defined by `doc/features/conversation-threads/design.md`.
- A Beryl home on native local NTFS is fully supported.
- A home on another filesystem or path is best-effort. Beryl opens it only when the location is
  usable and exclusive ownership can be established reliably. An unsupported or unusable location
  uses the startup home-failure behavior; a location owned by another live process uses the
  busy-home behavior.
- Every main window restored or created after a best-effort open shows the same home-location
  warning for that startup.

## Busy Home

- At most one OS process owns a Beryl home at a time.
- If another live process already owns the requested home, Beryl does not open the home, restore main windows, or offer lock takeover.
- Beryl presents one compact dedicated busy-home startup window rather than an ordinary main conversation window.
- The busy-home window exposes only one explicit command, `Exit`.
- The busy-home process exits automatically after approximately five seconds when the user does not exit sooner.

## Unreadable Store At Startup

- If the Beryl-home state store cannot be opened or validated at startup, Beryl does not claim that prior windows or threads were restored.
- Beryl does not reconstruct the session from CAS, a second snapshot, cached catalog rows, or filesystem guesses.
- No ordinary main conversation window is created from unvalidated session records.
- The startup failure remains explicit and bounded; it must not silently create a fresh replacement home over the unreadable store.
- Beryl presents the home failure window with `Retry` and `Exit`.
- Retry targets only the same configured home, reruns open and validation, and creates ordinary restored windows only after success.
- While that exact retry is pending, `Retry` remains visible but unavailable and repeated activation
  cannot create a duplicate attempt. `Exit` remains available.
- Closing the home failure window has the same outcome as activating `Exit`.
- A repeated failure keeps the home failure window and updates its bounded selectable detail. It never offers Reset, Choose Another Home, Take Over, or Continue Without History.
- Unlike the busy-home surface, unreadable startup does not auto-exit.
- The startup presentation retains at most 4,096 UTF-8 bytes of failure detail, including a visible
  truncation suffix when needed. It never retains an additional unbounded copy for display or
  clipboard use; repeated failure replaces the prior bounded detail without edit history.
- A Retry activation becomes pending before its event is delivered and carries an exact
  surface-local attempt identity. Only its matching failure completion may enable Retry and replace
  detail; stale completions cannot overwrite a newer attempt or revive a surface after Exit.
  Exit requests are idempotent and remain available while Retry is pending.
- Startup surfaces emit required Retry/Exit events to the process owner. Closing a surface requests
  Exit without removing it or quitting the application ahead of process-owned cleanup. Busy-home
  countdown uses one retained monotonic timer and requests the same Exit event at its deadline;
  disposal or earlier Exit cancels that timer. Presentation owns no home, service graph or recovery
  custody and does not start storage work.

## Startup Cleanup Blocked

- If failed startup cannot finish native, editor, command or home cleanup, show the dedicated
  startup failure window in an explicit blocked state. This is the exception to closing every
  attempted main window before failure presentation: remaining attempted windows retain their
  original ownership and stay noninteractive. Never present them as successful restoration.
- Show `Beryl couldn't finish starting`, bounded selectable diagnostic detail, and explain that
  cleanup could not finish and Retry is unavailable. Do not claim uncertain writes were saved,
  cancelled or rolled back, or display continuing progress for a terminal cleanup failure.
- Retry stays visible and disabled. Exit and native close retain their ordinary orderly-exit
  intent; they do not discard retained state, authorize another attempt, or force termination.
  An already requested Exit remains idempotent and does not prevent blocked failure presentation.
- Only this blocked startup state adds the startup surface's `Quit Anyway`; running-session final
  teardown follows the separate [main-window blocked-shutdown contract](../main-windows/design.md#final-teardown-and-blocked-shutdown).
  It is never the default focused action and requires
  explicit pointer or keyboard activation. Native close, Escape, timers and cleanup errors cannot
  activate it. No additional confirmation dialog is required because its consequence is shown
  beside the command: `Quit Anyway stops Beryl immediately. Unsaved changes may be lost; work
  already sent may still complete.`
- Quit Anyway terminates this application process without save, flush, ordinary shutdown or
  further recovery. It abandons in-memory cleanup and reconciliation custody, not by reporting
  success or deleting durable records. Existing OS-owned child-process containment still applies;
  termination does not prove remote work stopped or undo dispatched effects. A later manual launch
  validates durable state through ordinary startup. It neither restarts Beryl nor creates a panic
  report for this explicit user-requested termination.
- The blocked state is terminal for its startup attempt. Stale Retry completions cannot restore
  admission or overwrite its diagnostics. Quit Anyway admission emits at most one exact request.

## Persistent Store Failure During A Session

- A persistent state-store failure never closes, removes, replaces, repositions, or changes the virtual-desktop placement of an existing main conversation window.
- Every existing window keeps its position, size, virtual-desktop placement, selected thread identity, and last coherent resident surface in memory.
- Already resident content may remain readable, selectable, and copyable when those actions require no new durable read or mutation.
- When one mutation has an indeterminate durable outcome, the affected surface shows a reconciling
  state and disables only commands that depend on that result. It does not claim either the old or
  new state won. Unrelated coherent work remains available while store health permits it.
- Reconciliation may prove that the mutation did not commit, prove exact success, or return
  `Collision`, presented as terminal `Unavailable`, when neither result can be proved. Only proven
  noncommit restores the command on that basis, and only proven success publishes the new result.
  `Unavailable` does neither and never treats the retained prior presentation as proof that the
  mutation did not commit.
- A terminally unavailable mutation preserves the last coherent presentation plus its exact locally
  held intent and evidence, keeps duplicate or repeat mutation suppressed, and leaves only that
  request and actions that depend on its result unavailable. Its surface shows a persistent bounded
  explanation through the established disabled-control or notice treatment while unrelated healthy
  work remains available.
- The unavailable explanation may point only to established same-home recovery and bounded
  diagnostic reporting. Beryl exposes no operation-level retry, resubmission, rollback, or manual
  repair command for that exact mutation.
- History loading and navigation, thread activation, draft editing, submission, settings, window-session mutation, runtime/root mutation, metadata mutation, and every other operation requiring Beryl-home state are unavailable while the failure persists.
- While the failure persists, Beryl does not report a failed write as saved or present another
  history source as a substitute for durable Beryl history.
- Existing durable drafts and Syndic history remain preserved. Turn output that cannot be captured
  durably is never presented as canonical history; an affected active turn may become visibly
  repair-pending and later resolve as repaired or incomplete.
- Every affected main window presents the same home failure and recovery state through its own
  disabled controls and established error notice.
- An operation that would close a window or exit while changing the durable restore set does not complete when that restore-set write fails; the current windows remain intact.
- If the process terminates before recovery, only the last successfully committed session and window state can survive; the preserved in-memory state is not a redundant restore snapshot.

## Recovery Boundary

- These recovery rules apply to ordinary returned storage failures. An internal panic follows
  [fatal crash reporting](../crash-reporting/design.md), terminates the application and cannot
  enter in-process home recovery.

- Recovery must validate the same configured home and state store before any gated operation resumes.
- A retry never substitutes a new home, clears the existing home, drops records, or selects another thread merely to make the shell interactive.
- If recovery cannot prove a healthy store, the application remains in the preserved fail-closed state.
- Running-session recovery automatically retries and validates the same configured home. Every
  affected window reflects that shared progress and outcome.
- Same-home recovery does not turn a terminally unavailable mutation into success or proven
  noncommit, repeat it, or clear its unavailable explanation. It may restore unrelated healthy work
  without reopening that exact request.
- After validation succeeds, Beryl resolves every affected turn as repaired or explicitly incomplete
  before it reenables successor work for that thread.
- Each affected main window uses the established error notice when failure begins and an informational recovery notice after the same home validates successfully. No separate recovery window replaces a running conversation shell.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `availability-required/v1`

Full persistence guarantees apply to native local NTFS homes. Other accepted locations retain the
documented best-effort envelope, and an unreadable home at startup remains outside restoration
guarantees.
