# Goals

Own Beryl's integration boundary with `codex app-server`.

## Non-goals

- Owning GUI window state or rendering.
- Owning Syndic history, Beryl thread metadata, or home-state records.
- Owning Beryl-home thread catalogs, selected-thread restore, thread selector rows, or thread title authority.
- Owning shared UI model types that do not depend on backend integration.
- Owning durable Syndic transcript storage, transcript projection policy, or selected transcript rendering source decisions.

# Decisions

## Launch Ownership

- This crate owns managed host-Windows and WSL-Linux backend launch construction.
- Host-Windows launch executes the caller-supplied validated absolute Codex CLI path directly with `app-server` and never substitutes a `PATH` lookup.
- WSL-Linux launch targets `wsl.exe`, selects the caller-supplied validated distro, sets the requested working directory, and executes the caller-supplied validated runtime-native Codex CLI path directly with `app-server` inside WSL.
- Managed app-server launch targets an authenticated loopback WebSocket listener so multiple Beryl backend clients can connect to one Beryl-owned app-server process.
- Host-Windows managed WebSocket launch binds the app-server to `ws://127.0.0.1:<port>` in the requested execution root.
- WSL-Linux managed WebSocket launch binds app-server inside the selected distro and assumes the WSL loopback listener is reachable from host Windows on the selected localhost port.
- This crate owns choosing the managed listener endpoint, constructing the app-server auth flags, creating and cleaning up per-run capability-token files, and preventing raw auth tokens from appearing in process arguments or logs.
- This crate owns managed backend process supervision for every launch mode it constructs.
- Host-Windows managed launches are supervised as a Windows process tree so shutdown can terminate descendants as well as the immediate child process.
- WSL-Linux managed launches create a Beryl-owned cleanup boundary inside the selected distro so shutdown can terminate the Linux `codex app-server` process independently from the host `wsl.exe` wrapper lifetime.
- Managed backend shutdown is explicit, idempotent, waits for process exit with bounded escalation, and cleans per-run launch material after the process supervision boundary is released.
- Routine managed app-server stderr is debug-level diagnostic data rather than default-visible operator output. Failures in Beryl-owned stderr reading remain warnings.

## Protocol Boundary

- This crate owns transport I/O, compatibility probing, and normalization of backend events for the other Beryl packages.
- This crate supplies normalized CAS live events to the CAS-live Syndic transcript system defined in `doc/systems/cas-live-syndic-transcript/design.md`, but does not own Syndic durability, projection, or transcript presentation policy.
- Compatibility probing is based on the observed initialize handshake plus targeted request validation of required methods and fields.
- Managed WebSocket is the primary multi-client transport boundary.
- Stdio remains a single-client transport implementation for compatibility tests and fallback-oriented protocol work, but callers that require concurrent foreground and background backend operations must use independent WebSocket client sessions.
- This crate separates managed app-server process lifetime from backend client session lifetime.
- Dropping or closing a backend client session must not terminate a managed app-server process owned by a managed server handle.
- Each backend client session performs its own initialize handshake, request id sequencing, notification buffering, and stream polling.
- Each backend client session bounds deferred notification and server-request retention by message count and approximate retained bytes while it is waiting for a specific JSON-RPC response.
- Deferred dynamic tool-call requests have an additional explicit count cap. Exceeding that cap is a bounded-resource error instead of an unbounded retained request backlog.
- Backend client initialization requests the app-server experimental API capability when available, because Beryl depends on new protocol fields and notifications such as subagent `agentNickname` metadata and `thread/started`.
- Backend client initialization must not opt out of `thread/started` on sessions that can feed foreground turn-stream activity.
- WebSocket client sessions authenticate with the managed server using `Authorization: Bearer <token>` during the WebSocket handshake.
- The WebSocket transport layer owns the authenticated client handshake, outbound client-to-server masking, inbound frame-header parsing, opcode and reserved-bit validation, server-to-client masking rejection, continuation-frame state, control-frame handling, close handling, bounded handshake read-ahead retention, handshake timeout behavior, and bounded payload-byte reads.
- Stdio transport line reads are bounded before JSON parsing or stderr logging so a backend cannot force Beryl to retain an unlimited single stdout or stderr line.
- Protocol errors for oversized stdio lines and invalid JSON must not retain the rejected full line payload.
- WebSocket transport code must not know JSON-RPC method names, request ids, transcript item schemas, generated-image fields, or backend normalization types.
- The JSON-RPC session layer owns request id allocation, outstanding-method correlation, notification buffering, response routing, initialize handshake behavior, compatibility probing, and session-level cancellation semantics.
- Existing typed backend normalization remains the caller-facing boundary after transport reads and JSON-RPC routing complete.
- This crate does not expose Codex App Server thread-list normalization as a live public boundary. Beryl-home catalogs, selectors, restore paths, titles, runtimes, roots, and Syndic threads are not backend-discovered rows.
- Thread-start normalization exposes app-server ephemeral-thread support as an explicit backend protocol capability without deciding which GUI workflows may use it.
- Thread resume normalization may attach to an exact CAS projection for live execution or control, but it is not selected-thread activation or catalog proof.
- Thread read normalization is not a Beryl shell catalog, selector, restore, title, runtime, root, or Syndic-thread authority. Live GUI code must not use metadata-only reads to populate user-visible thread lists.
- Thread fork normalization exposes app-server `thread/fork` as creating a backend-owned conversation thread from an existing backend thread without deciding whether the GUI should activate the created thread or how downstream callers should present fork lineage.
- Thread rollback normalization exposes app-server `thread/rollback` as a backend-owned thread-history mutation targeted by exact thread id and trailing turn count without deciding whether GUI callers use it for branch preparation, source-thread editing, or another history-truncation workflow.
- Thread-item injection normalization exposes stable app-server `thread/inject_items` with an exact thread id and ordered raw Responses API item sequence, preserving item order and supported role/content shapes without starting or fabricating a Beryl turn.
- The normalized injection API accepts only the CAS-live system's closed canonical text-message subset: one user/input-text item or one assistant/output-text item per message, with no arbitrary raw item passthrough, extra fields, unknown types, or CAS-private wrapper conventions.
- Branch-selection projection uses the canonical assistant/output-text shape because the selected passage originates in assistant output. Recovery history preserves each source message's user or assistant role.
- The normalized injection boundary reports request success, structured rejection, transport loss, and unknown completion distinctly enough for callers to abandon an ambiguous fresh CAS thread rather than risk duplicate injection.
- One-time fresh execution-projection recovery crosses the injection boundary only after higher-level orchestration has selected a new empty CAS thread and assembled the exact Syndic-derived item sequence. This crate does not choose the sequence, recovery budget, native-lineage precedence, binding proof, or retry policy.
- Thread unsubscribe normalization exposes app-server `thread/unsubscribe` and its `notLoaded`, `notSubscribed`, and `unsubscribed` statuses so callers can unload a no-longer-needed loaded thread without treating it as persistent-thread deletion.
- Thread close notification normalization exposes app-server `thread/closed` as loaded-thread lifecycle state without deciding whether the closed thread was user-visible or a GUI maintenance thread.
- Thread-name setting normalization is not Beryl thread title authority. Generated Beryl titles are Beryl-home metadata unless a future design explicitly reintroduces backend name publication.
- Config-read normalization exposes app-server `config/read` for cwd-scoped model and reasoning configuration fields without deciding GUI fallback or presentation policy.
- Model-list normalization exposes app-server `model/list`, including model ids, display labels, hidden/default metadata, supported reasoning efforts, and default reasoning effort values without deciding how a GUI presents model selection.
- Supported reasoning-effort entries from `model/list` are normalized to stable effort identifiers for downstream callers across string, record, and keyed-map wire shapes. If app-server includes additional per-effort metadata, this crate may ignore that metadata until a caller-facing backend contract explicitly needs it.
- Thread-start and turn-start normalization expose app-server's ordered user input array, including text records, remote image URL records, and local-image path records, plus model overrides, reasoning-effort overrides, and hidden developer-instructions-capable payloads without deciding which caller workflows should use an override or how separate user input fragments are rendered.
- Developer-instructions payload normalization must preserve the caller-supplied developer-instructions text as hidden developer-instructions context rather than converting it into user input text or another transcript-visible record. The exact app-server request field may be a settings-shaped developer-instructions mechanism when app-server does not expose a standalone per-turn developer-instructions field.
- Turn-steer normalization exposes app-server `turn/steer` with thread id, expected active turn id, ordered text and image user input records, and returned turn id without deciding when GUI callers should steer an active turn or queue input for a later turn.
- App-server image input records do not provide a GUI-owned label field in the normalized backend contract. Callers that need model-visible names for images must send adjacent text input records that establish those labels.
- Local-image path normalization preserves caller-supplied paths as backend-runtime paths. This crate does not infer host-to-WSL path visibility or copy image files across runtime boundaries unless a later backend-boundary design explicitly adds that staging responsibility.
- Turn-start and turn-steer error handling must preserve enough structured app-server error information for callers to distinguish non-steerable active turns from transport failure or unrelated request failure when the protocol exposes that distinction.
- Turn-interrupt normalization exposes app-server `turn/interrupt` with exact thread id and turn id without deciding which GUI actions may request interruption.
- Hard-stop normalization exposes app-server execution-termination primitives for exact backend handles without deciding GUI stop policy. Supported hard-stop primitives include command-exec termination by backend process id, thread-scoped background-terminal cleanup, and exact turn interruption for associated child or subagent turns when callers provide exact thread id and turn id.
- Hard-stop normalization must preserve request-level failure information per target so callers can report partial hard-stop success rather than collapsing all escalation outcomes into one opaque transport error.
- Thread-compaction normalization exposes app-server `thread/compact/start` as a thread-id-targeted backend operation without owning the GUI policy for when users may request compaction.
- Token-usage normalization exposes only app-server-provided exact token usage from stream notifications or read-only protocol responses.
- Account rate-limit normalization exposes only app-server-provided exact account rate-limit snapshots from stream notifications or read-only protocol responses, including the multi-bucket account rate-limit read response and backend bucket identity fields such as `limitId` and `limitName` when the protocol provides them.
- If app-server exposes latest per-thread token usage through read-only thread metadata, this crate owns normalizing that field without making GUI callers depend on raw protocol JSON.
- This crate must not estimate status-line context from transcript text or local tokenization.
- Turn-stream normalization exposes backend thread-started notifications and thread status updates with enough structure for callers to observe backend-provided thread metadata such as subagent nicknames, including top-level nickname fields and nested subagent source metadata, and to distinguish idle, active, system-error, and not-loaded loaded-thread states.
- Normalized foreground turn-stream events are available for CAS-live Syndic ingestion. This crate does not commit those events to Syndic, choose durable chunking, decide transcript-view projection policy, or own the ingestion invariants defined in `doc/systems/cas-live-syndic-transcript/design.md`.
- Turn-stream normalization exposes backend-provided subagent label updates from raw collab-agent spawn completion notifications when the app-server stream provides a spawned thread id and nickname before the corresponding thread metadata is observed.
- Turn-stream normalization may expose backend thread-name update notifications as raw live events, but Beryl shell title authority must not depend on them.
- Turn-stream normalization exposes backend activity with stable stream identity, including thread id, turn id, item id, raw protocol item type, raw command text for command-execution items when the protocol provides it, raw tool-name fields when the protocol provides them, raw item status when the protocol provides it, lifecycle status, summary-only reasoning update detail when the protocol provides it, exact collab-agent spawn model/reasoning metadata when a collab-agent item provides it, file-change summary counts derived from explicit `fileChange` records when the protocol provides them, and the raw file-change path only when those explicit records identify exactly one unique path.
- Turn-stream normalization preserves backend-exposed hard-stop handles on operational tool activity, such as command-exec process ids, as opaque backend handles. This crate must not synthesize hard-stop handles from command text, working directory, or local process inspection.
- Native app-server execution items, dynamic tool calls, collab-agent tool calls, external MCP tool calls, and reasoning activity remain distinct normalized activity sources; this crate must not treat external MCP server inventory as the universal registry for app-server activity.
- Dynamic tool-call normalization preserves app-server namespace, tool name, call id, thread id, turn id, arguments, and response transport without interpreting Beryl-owned dynamic tool semantics such as lifecycle yield outcomes.
- Reasoning activity normalization may expose reasoning item lifecycle and backend-provided reasoning summary text, but it must not expose raw reasoning content or `item/reasoning/textDelta` payloads as activity detail.
- Collab-agent tool-call normalization exposes receiver thread ids, target-agent status metadata, and optional spawn model/reasoning fields as raw activity metadata without deciding how the GUI presents them. Missing spawn model/reasoning fields remain absent rather than inferred from configuration defaults, model-list defaults, parent-thread state, receiver thread ids, `agentsStates`, nicknames, or caller state. Spawn model/reasoning item metadata remains distinct from exact child-thread runtime model/reasoning metadata exposed through thread metadata responses when the protocol provides those fields. Subagent nicknames come from backend thread metadata when the protocol provides them.
- Activity normalization does not synthesize human-friendly labels, inspect command arguments for display names, or decide GUI visibility, retention, sorting, command-line truncation, or log presentation policy.
- Turn-stream normalization exposes app-server approval server requests with enough structure for callers to deny them and, when necessary, interrupt the associated turn without depending on raw protocol messages.
- Approval response normalization can send protocol-specific denial responses for command-execution, file-change, and permission-expansion approval requests without deciding when a GUI should deny a request.
- Turn-stream normalization distinguishes idle receive polling from fatal stream failure. A quiet interval returns no event while transport errors, backend process exit, protocol errors, and invalid notifications remain explicit failures or events for callers to handle.

## Dependency Boundary

- This crate must not depend on `gpui`.
- Shared Beryl identity and presentation values consumed across crates belong in `beryl-model`.
