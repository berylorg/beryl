# Goals

Support bounded operator debugging, live testing, and resource investigation through Beryl-owned diagnostic tools and isolated diagnostic child processes.

## Non-goals

- Exposing arbitrary process launching through diagnostic child control.
- Mutating backend conversation history, semantic graph state, workspace persistence, settings, transcript content, or durable image assets through observation tools.
- Loading additional transcript history, decoding media, or creating renderer resources solely to answer diagnostics.

# Decisions

## Supervisor Diagnostics

- Beryl may expose Beryl-owned app-server dynamic tools on Beryl-created conversation threads for operator-approved debugging, live testing, and resource investigation.
- Supervisor self-observation tools may report bounded snapshots of Beryl process identity, platform memory counters, active workspace/thread identity, managed backend child process ids, retained GUI projections, visible transcript media, and recent media lifecycle events.
- Diagnostic output must be bounded by deterministic item and byte caps. Large strings, paths, labels, error text, media keys, event details, and lists are truncated or omitted rather than returned without limit.
- Diagnostics must not retain image bytes, decoded pixel buffers, GPUI image handles, process handles, backend responses, or transcript payloads solely to make later diagnostics more detailed.
- Recent diagnostic event logs are metadata-only bounded rings.
- Process and memory diagnostics may expose platform counters. Windows builds should expose GUI process id, known child backend process ids, Private Bytes, Working Set, commit-related counters when available, handle count, and thread count. Unsupported counters are reported unavailable rather than guessed.
- Visible-media diagnostics report only currently retained or visible GUI projection state. They must not load history pages, read files, decode images, or create GPUI assets just to answer.
- Diagnostic child `read_ui_state` reports the latest retained selected-parent turn as exact thread id, turn id, and terminal status only when the latest retained turn is completed, interrupted, or failed and both identities fit the diagnostic identity-field bound. It does not substitute an older terminal turn while a newer selected turn is queued, starting, or running.
- Diagnostic child `read_ui_state` may report a bounded selected-thread sample of retained multi-agent v2 lifecycle activity. Each sample record contains only the exact parent event thread, turn, and item ids; optional exact child subject thread id; normalized lifecycle kind; row status; and nickname-resolution state.
- Multi-agent v2 diagnostic activity excludes legacy collaboration records, labels, display values, prompts, reasoning, tool output, model metadata, reasoning-effort metadata, and `agentPath`. Resolved nickname state means exact backend thread metadata supplied the nickname; activity-derived or display labels do not count as resolved.
- The multi-agent v2 activity sample is a deterministic newest-prefix projection of records visible for the selected thread. It retains immediate parent event identity for nested activity rather than rewriting records to a root identity, and it never retains state solely for diagnostics.
- The sample is capped at 64 records, 64 KiB of aggregate identity text, and 512 UTF-8 bytes per exact identity field. A record with a missing required identity or an over-bound identity is omitted; item-cap, byte-cap, and invalid-identity omission set an explicit truncation flag. No selected thread produces an empty, non-truncated sample.

## Diagnostic Child Process

- Beryl may expose a dot-free diagnostic child-control dynamic-tool namespace implemented by the supervisor Beryl.
- The supervisor controls at most one diagnostic child process at a time.
- A diagnostic child runs with an explicit Beryl home directory distinct from the supervisor home. Launching a child against the supervisor home is rejected.
- Diagnostic child startup uses the supervisor's current executable by default, or a caller-supplied explicit compatible Beryl executable when the operator needs another build.
- An explicit executable path is diagnostic child identity. It is spawned directly without shell interpretation and must not turn child control into arbitrary command-line or environment mutation.
- Startup reporting includes the actual executable path when available.
- Starting a custom executable must prove the child supports Beryl's diagnostic-target stdio mode before reporting it as started.
- Unsupported, incompatible, missing, or non-executable targets fail with bounded diagnostic errors and must not leave unmanaged child lifecycle.
- If cleanup after failed startup cannot complete immediately, the supervisor retains ownership for status and later stop retry.
- Supervisor-to-child control uses a Beryl-owned bounded local control protocol over child stdio. Child stdout is reserved for protocol frames; logs use stderr or files.

## Diagnostic Child Controls

- Diagnostic child controls are supervisor dynamic tools for testing an isolated child Beryl instance. They are not visible end-user controls in the ordinary workspace screen.
- Child control may switch workspaces or threads, list workspace threads from bounded child-owned inventory state, select a pending new-thread draft, submit bounded text through the child composer, request soft or hard stop for the child's exact selected active turn, scroll transcript, close transient popups, and wait for bounded UI or turn-state predicates.
- Child commands must drive the same internal application command paths and state transitions as corresponding visible UI interactions or retained UI projections.
- Child commands reject ambiguous, stale, missing, or unavailable targets and report timeout or partial state instead of blocking indefinitely.
- A child command must not fall back to another workspace, thread, runtime target, turn, stop target, or input path when the requested target cannot be used exactly.
- Child composer submission may synthesize user-authored transcript input only for the isolated child and only through ordinary validation, draft acceptance, transcript insertion, new-thread creation, active-turn steering, compaction queueing, and rejection behavior.
- Child soft stop uses the same exact selected-thread active-turn interruption behavior as the visible status-line `Soft stop` action.
- Child hard stop uses the same exact selected-turn interruption and backend-exposed hard-stop targets as the visible popup, but the diagnostic request supplies deliberate activation instead of the visible three-second hold affordance.
- Stop commands may interrupt only the child's exact selected active backend turn or selected active compaction operation when Beryl knows the interruptible backend turn id.
- Hard stop must use only exact backend-exposed termination handles and never guessed OS pids, process names, working directories, or local process trees.
- Child wait commands observe bounded UI state without creating loading UI solely for diagnostics and return latest bounded state on timeout.
- Diagnostic child UI-state and command results report backend-unavailable workspace state and backend-unavailable submission/thread-listing rejection distinctly from ready, opening, workspace-idle, and blocked shell states.

## Boundaries

- Diagnostic child GUI-control commands must not edit backend history, apply settings, mutate semantic graph data, mutate the supervisor instance, bypass availability checks, or use direct app-server calls when a child-owned UI/application path defines the behavior.
- Diagnostic child control is independent of the child app-server turn lifecycle. Supervisor commands reach the child over the child control channel rather than through that child's dynamic-tool stream.
- Beryl-owned dynamic-tool results use the existing app-server dynamic tool-call response contract for the supervisor-facing call. The child-control protocol remains an internal Beryl process boundary.
