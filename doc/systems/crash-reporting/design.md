# Goals

Terminate a panicked application without relying on its shared-state cleanup, while retaining a
small independent diagnostic surface when the operating environment permits it.

## Non-goals

- Whole-process cooperative stopping, arbitrary thread suspension, or panic recovery.
- Crash databases, uploads, restart supervision, general IPC, or a cross-platform process framework.

# Decisions

## Ownership And Startup

- The [feature](../../features/crash-reporting/design.md) owns visible behavior. The `beryl`
  executable selects ordinary or reserved reporter mode before ordinary bootstrap. Reporter mode
  uses the same executable and never opens Beryl-home, constructs domain services, launches a
  backend, installs ordinary application hooks, or starts another reporter.
- `beryl-app` owns the bounded reporter channel and GPUI report surface. The composition root
  establishes fatal handling before storage open or ordinary worker startup. A fallback abort-only
  hook covers unavailable reporter setup; successful setup replaces it with the report-and-abort
  hook. Ordinary runtime code cannot replace the installed fatal hook.
- At most one reporter is started for one application instance. It waits without a visible window
  or polling until the exact application process exits. No reporter restart or reconnect exists.
- The Windows implementation passes only a fixed anonymous pagefile-backed mapping, an exact
  synchronization-only parent process handle, and a startup-ready event through an explicit handle
  inheritance list. No process-id lookup, named endpoint, listening socket, report file, or home
  handle participates. Handles not on that list are not inherited.
- The reporter must survive application-owned job termination. Launch requests job breakaway;
  readiness is refused if the reporter remains in a job. Unavailable breakaway is ordinary reporter
  setup failure, not permission to detach, reparent, or introduce another launcher.
- Startup readiness has a five-second upper wait bound. Failure releases the channel and stops and
  reaps the exact provisional child. No retry occurs; the abort-only hook remains authoritative.

## One Report And Fatal Termination

- The report contains at most 4,096 UTF-8 bytes including version, source location, payload and
  explicit truncation indication. Its one fixed shared-memory record contains a format identity,
  completion marker, length and text. It is not an extensible message protocol or queue.
- Only the first panic may write that record. Repeated or concurrent panic takes the abort-only
  path. One atomic writer claim has no application retry loop; the aligned completion word uses
  release/acquire-equivalent publication over the shared mapping. The hook reads only string
  payload and source-location facts and appends them into fixed
  storage; it invokes no arbitrary payload formatting, backtrace capture, storage API, logging,
  GUI callback, explicit lock, allocation, I/O, or wait of its own.
- After publishing a complete record, the hook invokes direct process abort and never returns to
  unwinding, catches, application acknowledgements, or normal shutdown. No cleanup or flush is
  required for report delivery. Rust/runtime/OS behavior before or during the hook is outside a
  hard real-time guarantee; failure to reach or finish it may lose the report.
- The reporter waits on the exact inherited process handle, not a reusable PID. Only after process
  exit does it read and validate a complete bounded record, copy it into reporter-owned memory,
  release all channel handles and mappings, and construct the report window. An absent, incomplete,
  malformed or incompatible record yields silent reporter exit.
- Startup uses a real duplicated parent handle, not a pseudo handle. Temporary inheritable
  parent-side copies close after child creation, and the child clears inheritance on its received
  handles before readiness. The installed hook retains its mapped view through late shutdown.
- An ordinary parent exit without a complete report automatically releases the waiting reporter.
  The parent does not join a reporter that is waiting for parent exit. The reporter holds no
  application state, storage ownership, execution capability, or handle that keeps the parent alive.
- Reporter-mode panic directly aborts without recursive reporting. Unsupported platforms install
  abort-only handling. The Windows report boundary does not weaken normal operation elsewhere.

## Scope Of Termination

- Fatal handling replaces application panic recovery; ordinary returned-error recovery remains in
  its owning systems. Lower-level unwind tests may exercise library cleanup without establishing
  application recovery policy.
- Process termination stops the failed application's execution. Existing OS-owned backend process
  lifetime containment applies; fatal handling does not run turn-stop, replay, cancellation or
  compensating commands. Already dispatched effects are not undone or classified from exit alone.
- The reporter is solely a local error surface. It has no authority to reopen data, restart work,
  inspect another process, or retain a diagnostic history.

## Acceptance And Complexity Boundary

- Use process tests for real panic-to-abort delivery, silent normal exit, setup refusal/timeout,
  exact inherited handles, job-breakaway refusal, incomplete/oversized records, repeated panic,
  reporter failure and release of waiting children. Verify capture performs bounded app-owned work
  and the reporter releases transport before presentation.
- Verify the GUI and final executable mount separately. The final mount must exercise ordinary
  startup hook ordering and a real isolated GUI report after application process death; helper
  tests alone do not establish that production integration.
- Keep implementation in focused modules of existing packages, use existing dependencies, and add
  no reusable crash framework. Any additional process, persistence, retry, watchdog, telemetry,
  recovery mechanism or supported-platform implementation is a separate architectural decision.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `sensitive-data/v1`

Independently review handle ownership, panic-path bounds, separation from failed state and report
export. A missing report is an allowed failure. The report channel must not extend application
execution or leak ordinary inherited capabilities into the reporter.
