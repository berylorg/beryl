# Goals

Support bounded operator debugging, live testing, and resource investigation through Beryl-owned diagnostic tools and isolated diagnostic child processes.

## Non-goals

- Exposing arbitrary process launching through diagnostic child control.
- Mutating Syndic history, Beryl-home state, settings, transcript content, or durable image assets through observation tools.
- Loading additional transcript history, decoding media, or creating renderer resources solely to answer diagnostics.

# Decisions

## Supervisor Diagnostics

- Beryl may expose Beryl-owned app-server dynamic tools on Beryl-created conversation threads for operator-approved debugging, live testing, and resource investigation.
- Supervisor self-observation tools may report bounded snapshots of Beryl process identity, platform memory counters, selected window/thread/runtime identity, managed backend child process ids, retained GUI projections, visible transcript media, and recent media lifecycle events.
- Diagnostic output must be bounded by deterministic item and byte caps. Large strings, paths, labels, error text, media keys, event details, and lists are truncated or omitted rather than returned without limit.
- Diagnostics must not extend the lifetime of media, child activity, backend results, or transcript
  content solely to make later diagnostic output more detailed.
- Recent diagnostic events expose only a bounded metadata-only history.
- Collecting diagnostics must not introduce expensive work into ordinary rendering or scrolling.
- Process and memory diagnostics may expose point-in-time platform counters. Windows builds should
  expose GUI process id, known child backend process ids, Private Bytes, Working Set, commit-related
  counters when available, handle count, and thread count. These observations do not attribute
  process-wide usage to individual Beryl queues, caches, windows, turns, CAS work, renderer
  resources, or allocator overhead and are not exact reconciliation against every allocation.
  Unsupported counters are reported unavailable rather than guessed.
- Visible-media diagnostics report only media already retained for or shown in the current
  presentation. They must not load history pages, read files, decode images, or prepare new media
  solely to answer.

## Diagnostic Child Process

- Beryl may expose a dot-free diagnostic child-control dynamic-tool namespace for isolated child testing.
- The supervisor controls at most one diagnostic child process at a time.
- A diagnostic child runs with an explicit Beryl home directory distinct from the supervisor home. Launching a child against the supervisor home is rejected.
- Diagnostic child startup uses the supervisor's current executable by default, or a caller-supplied explicit compatible Beryl executable when the operator needs another build.
- An explicit executable path is diagnostic child identity. Choosing one does not expose arbitrary
  commands, arguments, or environment mutation through diagnostic child control.
- Startup reporting includes the actual executable path when available.
- Starting a custom executable must prove that it supports Beryl's diagnostic child mode before
  reporting it as started.
- Unsupported, incompatible, missing, or non-executable targets fail with bounded diagnostic errors
  and are never reported as started.

## Diagnostic Child Lifecycle

- `Start child` is available only when no diagnostic child or unfinished child-disposal attempt is
  owned. Acceptance reports `starting`; status remains available until the exact request reports
  `running` or `failed`, and repeated start does not create another child.
- Status reports bounded child identity, home, executable, readiness, and latest observable outcome.
  It distinguishes `no child`, `starting`, `running`, `stopping`, and `failed`
  rather than guessing readiness from a timeout or incomplete observation.
- `Stop child` requests orderly disposal of the one exact diagnostic child. Repeated stop joins the
  same attempt. Status reports `stopping` until disposal succeeds, then reports `no child` and makes
  a later explicit start available.
- Startup, unexpected-exit, or disposal failure reports `failed` with a bounded reason. When any child
  activity or disposal obligation remains, new start stays unavailable and `Stop child` remains
  available to join or retry orderly disposal. Once no child activity or disposal obligation
  remains, status reports `no child`, retains the latest bounded outcome for inspection, and makes
  a later explicit start a new request.
- Orderly diagnostic-child disposal is separate from interrupting the child's selected CAS turn or
  compaction. It is allowed lifecycle control for the owned diagnostic child, not a hard stop,
  coarse stop, escalation, or arbitrary process-termination control for conversation work.

## Diagnostic Child Controls

- Diagnostic child controls are supervisor dynamic tools for testing an isolated child Beryl instance. They are not visible end-user controls in the ordinary conversation shell.
- Child control may switch threads, list threads from the bounded Beryl-home catalog, invoke ordinary New Thread, submit bounded text through the child composer, request soft stop for the child's exact selected active turn, scroll transcript, close transient popups, and wait for bounded UI or turn-state predicates.
- Child commands produce the same validation, availability, and visible outcomes as corresponding
  ordinary UI interactions.
- Child commands that activate image-heavy or history-heavy transcript states exercise real child UI work and must honor the transcript feature's residency and presentable-media admission gates. They must not use diagnostic shortcuts that publish unloaded, media-pending, or otherwise non-presentable transcript rows.
- Child commands reject ambiguous, stale, missing, or unavailable targets and report timeout or partial state instead of blocking indefinitely.
- A child command must not fall back to another thread, runtime, root, turn, stop target, or input path when the requested target cannot be used exactly.
- Child composer submission may synthesize user-authored transcript input only for the isolated child and only through ordinary validation, draft acceptance, transcript insertion, new-thread creation, active-turn steering, compaction queueing, and rejection behavior.
- Child soft stop uses the same exact selected-thread active-operation interruption behavior as the
  visible status-line `Soft stop` action.
- `Soft stop` may interrupt only the child's exact selected active backend turn or selected active
  compaction operation when that exact target is available. It is the only diagnostic
  control that interrupts CAS work. Diagnostic control exposes no hard stop, escalation, or
  background-cleanup fallback for that work and never substitutes an inexact target.
- Child wait commands observe bounded UI state without creating loading UI solely for diagnostics and return latest bounded state on timeout.
- Diagnostic child UI-state and command results report home failure, zero-runtime, runtime/CAS unavailable, submission-disabled, catalog-loading, selected-thread-active, and idle states distinctly.

## Boundaries

- Diagnostic child GUI-control commands must not edit history directly, apply settings, mutate
  Beryl-home records directly, mutate the supervisor instance, or bypass ordinary validation and
  availability behavior.

# Engineering Rigor

Profile: `trusted-internal-tool/v1`

Modifiers:

- `untrusted-input/v1`
- `privileged-access/v1`
- `external-side-effects/v1`

Human operators are trusted. Dynamic-tool requests and caller-supplied executable paths are
untrusted, and the supported operating envelope contains one isolated diagnostic child.
