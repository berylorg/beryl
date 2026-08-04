# Goals

Define Beryl's internal backend runtime system for launching, probing, connecting to, supervising, and recovering `codex app-server` runtime targets.

Preserve exact runtime, root, backend-process, Syndic-thread, CAS-thread, turn, auth, policy, sandbox, and protocol boundaries while allowing user-visible features to degrade predictably when a runtime is unavailable.

## Non-goals

- Defining user-visible recovery copy, disabled states, or runtime/root workflows that belong in feature docs.
- Bundling, installing, replacing, or modifying Codex.
- Exposing operator-managed unauthenticated app-server listeners.
- Treating backend thread enumeration as the source of Beryl thread, runtime, or root identity.
- Owning Syndic durable transcript history, transcript projection, or transcript presentation policy.

# Decisions

## Runtime Ownership

- Beryl integrates with Codex through `codex app-server` as an out-of-process client.
- Beryl launches and owns managed backend processes in V1. It does not attach to already running app-server instances.
- One configured runtime is identified by one canonical Codex CLI executable path plus its derived Host or exact WSL-distribution mode. Runtime identity is not inferred from `PATH` and is not merely the environment name.
- Runtime admission canonicalizes the selected file, derives Host or WSL distribution from its filesystem boundary, converts WSL selection paths to an exact runtime-native path, and rejects paths whose executable or environment identity cannot be proven.
- Backend availability is tracked per configured executable runtime, not globally, not merely per Host or WSL environment, and not per root.
- A backend-unavailable runtime disables backend-required operations for threads bound to it without erasing runtime/root records, rebinding threads, changing window selection, or requiring application exit.
- One backend-unavailable executable runtime does not disable another configured executable runtime, including another path in the same Host or WSL environment.

## Launch And Listener Security

- Host-Windows launch executes the runtime's exact configured Codex CLI path directly with `app-server`; it does not resolve `codex` from the process `PATH`.
- WSL launch uses `wsl.exe`, targets the runtime's derived distro, sets the requested working directory, and executes the runtime's exact configured runtime-native Codex CLI path with `app-server` inside that distro.
- Every managed launch enables strict configuration and requires the effective nested CAS settings
  `features.multi_agent_v2.enabled = true` and
  `features.multi_agent_v2.expose_spawn_agent_model_overrides = true`. Release defaults and launch
  arguments alone are not compatibility authority. Beryl supplies them as one atomic SessionFlags
  table override so a later scalar feature toggle cannot replace the configured object.
- A Beryl-owned app-server listens on an authenticated loopback WebSocket endpoint chosen by Beryl.
- Beryl generates a high-entropy capability token per managed launch, stores it only in a per-run local token file and memory, passes the token file to app-server auth configuration, uses the token in WebSocket handshakes, and removes the token file when the server exits.
- Managed listeners bind only to loopback addresses and must not expose unauthenticated non-loopback endpoints.
- Cross-boundary communication uses the app-server contract rather than direct access to Codex storage or process memory.
- The managed process owner mints the only production client connectors for its authenticated
  listener. Each connector carries opaque launch provenance tied to that exact process boundary,
  runtime identity, executable paths, runtime mode, and working directory;
  caller-supplied endpoints, bearer values, executable labels, or detached probe reports cannot
  manufacture production admission authority.
- A thread's exact runtime/root binding supplies the CAS working directory for its execution projection. CAS alone applies working-directory-dependent `AGENTS.md`, skill discovery, sandbox, configuration, and instruction behavior; Beryl neither pre-reads nor emulates those rules.

## Backend Lifecycle

- Launching a backend through a local child process, including through `wsl.exe`, is a supported lifecycle mode.
- The GUI owns every backend child process it launches and terminates those processes when no longer needed or when the GUI exits.
- Managed termination covers the supervised runtime boundary, including descendants that would otherwise outlive the immediate child.
- Host-Windows launches are supervised as Windows process trees.
- WSL launches create a Beryl-owned cleanup boundary inside the selected distro.
- Ordinary close of one main window releases only that window's runtime interest. A managed backend remains alive while another window or in-flight operation requires it.
- Dedicated application Exit and final process shutdown explicitly stop active managed backend processes before process exit. Destructors are fallback only.
- Backend process lifetime is separate from backend client connection lifetime.
- Dropping one client connection must not terminate a managed app-server still needed by other work.
- One Beryl process may manage backend processes for multiple runtimes as needed, but uses at most one active managed app-server process for a given runtime.
- Foreground turns, thread activation, inventory refresh, title generation, status operations, and lazy maintenance use independent backend client connections when sharing one connection would delay foreground streaming or completion.
- A foreground candidate connection is created with immutable full-profile intent and its
  configured parser, page, pre-bind control, queue, and concurrency limits before initialize reads
  its first byte. Background request-only clients use a distinct bounded response policy and cannot
  be promoted into a foreground stream connection after construction.
- Status model discovery keeps `model/list` cursor pagination intact. One popup query owns a
  runtime-session generation, bounded resident result pages, and one continuation request; popup
  close, runtime change, or stale generation cancels it. The runtime manager never aggregates all
  pages into a process cache.

## Progressive Warm-Up

- Opening the Beryl home and creating restored conversation shells does not launch CAS.
- After shells exist, Beryl requests warm-up only for each unique runtime needed by a currently open window's selected thread.
- One process-wide runtime manager coalesces concurrent warm-up requests for the same runtime into one launch/probe lifecycle and fans the resulting readiness state out to every interested window.
- Thread-catalog membership alone never warms a runtime.
- Cancelling one window's interest does not cancel a shared launch still required by another window.
- Runtime launch, probe, retry, and shutdown work remains off the GPUI thread and uses bounded request and process timeouts.

## Neutral Maintenance Roots

- Maintenance turns that must avoid project instructions use a runtime-local empty directory reserved for the managed CAS lifecycle.
- Each Host runtime uses its own empty directory under the Beryl home sidecar area, keyed by a non-secret digest of configured executable identity.
- Each WSL runtime uses an empty per-Beryl-home-and-runtime directory under that distribution's user cache directory, keyed by non-secret digests of canonical Beryl-home and configured executable identities.
- The WSL directory contains no durable Beryl authority. It is recreated or validated as empty before use and may be removed when the runtime shuts down.
- Beryl does not fall back to a project root when neutral-directory creation or validation fails; the maintenance operation is unavailable for that runtime.

## Capability Probing

- Runtime admission performs a bounded executable/version and required-capability probe against the exact selected path before the runtime and its home root may commit. Failure returns a typed admission error and leaves no partial runtime record or managed process.
- Beryl probes backend compatibility when a managed backend is launched or explicitly probed for a runtime target.
- Compatibility probing validates the required app-server contract for the current Beryl target version and capability set.
- Required capabilities include exact thread resume by id for approved live execution/control, live
  turn event streaming suitable for Syndic capture, model listing, cwd-scoped config reads, ordered
  text and local-image user input on turn start or steering, developer-instructions payloads,
  active-turn steering by expected turn id, active-turn interruption, thread compaction, dynamic
  tool registration, and reverse dynamic tool calls. Runtime-readable files and transcript media
  use Beryl's bounded filesystem and asset boundaries rather than app-server `fs/readFile`.
- The fixed steering compatibility probe is a private non-destructive absent-target request with
  one minimal valid text input and an explicit expected turn id. The exact recognized rejection
  proves that the pinned method and required parameter names reached the loaded-thread lookup; it
  neither executes production steering nor substitutes for the public runtime operation.
- Operational steering admission separately combines that probe with the exact-version gate and
  retained 0.146.0 evidence for the specialized public boundary: exact thread and expected active
  turn identities, a bounded caller correlation, streamed ordered text and local-image input, an
  exact `{turnId}` success response, and delayed user-message lifecycle correlation. A detached
  probe fact alone never authorizes a foreground steering dispatch.
- Compatibility probing consumes initialize, config, one model page, and method success or rejection
  through their schema-specific bounded decoders. It retains no model-page aggregate, raw response,
  arbitrary error data, or incidental configuration.
- Exact CAS 0.146.0 compatibility requires the probed session to come from the managed process
  owner's production connector and requires its effective config read to prove both native
  multi-agent settings enabled and both effective dotted origins owned by `sessionFlags`. Missing,
  false, malformed, superseded, or otherwise unprovable settings reject the runtime; a lifecycle-
  test connector is not production authority.
- Required capabilities include exact native CAS continuation and fork plus stable 0.146.0
  `thread/inject_items` with the recovery semantics defined by
  `doc/systems/cas-live-syndic-transcript/design.md`.
- Foreground-stream compatibility is release-scoped. Exact CAS 0.146.0 admission requires retained
  proof of
  uninterrupted full-notification subscription, serial item-before-terminal FIFO consumption for
  normally finishing ordinary turns, a
  closed disposition for every public pinned item variant, typed item-delta discrimination, and
  fail-closed loss handling because reconnect, resume, late subscription, and process restart do
  not replay notifications. A CAS-version change must refresh the retained source and installed
  runtime proofs before admission can rely on equivalent semantics.
- Hosted Responses image generation is not an admitted exact-CAS-0.146.0 producer capability unless
  retained release-scoped evidence proves that the client can send the native `image_generation`
  tool declaration. Standalone
  `image_gen.imagegen` is the admitted image-generation path and is probed and normalized
  separately. Parser tolerance for an unsolicited hosted item from a nonconforming custom provider
  does not admit that provider behavior.
- Branch execution additionally requires the exact one-time selected-context projection proven by `doc/systems/cas-live-syndic-transcript/design.md`; schema presence without accepted-limit and trust-semantics proof is insufficient.
- CAS thread-list, CAS historical turn reads through `thread/turns/list`, and full-history `thread/read` are not Beryl catalog or transcript capabilities. Compatibility probing must not require them, and live backend code must not retain them as a fallback surface.
- Branch-discussion creation is Syndic-native and performs no CAS work before first user submission. Its later execution prefers exact CAS-native inherited parent context and uses fresh recovery injection only when that lineage is unavailable or unprovable.
- Edit actions depend on app-server rollback and turn-start primitives plus exact rollback-scope proof. When missing or unprovable, edit actions are disabled rather than emulated.
- Hard-stop backend primitives are admitted separately. Non-destructive primitives may use typed
  compatibility probes; destructive coarse cleanup requires exact pinned-source evidence plus
  negotiated experimental capability and is never invoked merely as a probe. Missing hard-stop
  support disables only affected escalation targets and must not disable soft interruption.

## Exact Interruption Boundary

- Active-turn interruption is a required soft-stop capability. Production use requires the exact
  loaded foreground session, CAS thread id, CAS turn id, runtime generation, managed-process
  generation, and loaded-thread generation already proven by the CAS-live target. A request-only
  client, newly resumed session, status string, or process lookup cannot substitute for that
  authority. The pinned empty-`turnId` startup-cancellation mode is never an exact turn operation
  and is rejected locally.
- The sole foreground connection driver serializes `turn/interrupt` with stream polling, approval
  responses, target closure, and terminal handoff. It exposes two non-interchangeable authority
  families over that one wire method. Durable stop carries the durable stop-operation and claimed-
  attempt correlations. Persistent-store-failure interruption instead carries one volatile,
  process-local failure-attempt correlation and cannot be passed to a durable stop or cleanup
  method. Every correlation remains local and is never fabricated as app-server idempotency
  support.
- Exact CAS 0.146.0 interruption admission requires retained source and live evidence of whether the
  checked app-server turn is also the target of the submitted core interrupt. Unless that evidence
  proves one atomic targeted primitive, Beryl treats the core interrupt as untargeted. Production
  exactness therefore additionally requires Beryl's exclusive authenticated managed listener and
  a target-operation fence that prohibits a
  successor turn or compaction start from the handler precheck through request disposition or
  terminal observation. Without that proof, the interruption capability is unavailable.
- The normalized request outcome is closed: matching response acceptance,
  `RejectedBeforeCoreInterrupt`, local proven non-dispatch, or completion unknown after possible
  dispatch. Local proven non-dispatch requires byte-level writer evidence or rejection before
  transport. Timeout, malformed matching response, response-identity failure, transport loss after
  any request byte may have crossed, and connection loss before a matching response are completion
  unknown. Publishing completion unknown retires that exact session before another request or
  poll.
- Under exact CAS 0.146.0, a correlated `-32600` response with absent `data` or the handler-local
  `-32603` submission-failure response with absent `data` normalizes to
  `RejectedBeforeCoreInterrupt` only after retained exact-release source evidence proves that the
  handler did not enqueue a core interrupt. The version-scoped verdict proves only that nondispatch
  fact. It supplies neither a machine-readable cause nor a verdict that the
  requested target remains current. Diagnostic text and arbitrary JSON-RPC error data are never
  target verdicts. The app may safely reopen only from separate local proven-nondispatch evidence
  while exact target authority remains; handler rejection instead requires terminal evidence or
  retirement of the uncertain projection.
- The backend boundary never retries interruption. A matching response proves request acceptance,
  not interrupted lifecycle or terminal history. Exact turn-stream terminal evidence, target
  closure, or connection-authority loss remains a separate ordered observation. A volatile
  persistent-failure outcome supplies no durable admission, stop receipt, retry authority, failure-
  generation proof, or target-selection claim; Beryl-home failure policy and attempt election stay
  outside the backend boundary.
- Hard-stop methods are optional per primitive and use exact opaque handles supplied by normalized
  activity on the same target generation. Turn-process termination, associated child or subagent
  turn interruption, and supported thread-scoped background-terminal cleanup each return their own
  matching response, source-pinned rejection, proven-nondispatch, possible-dispatch, or unsupported
  outcome. One target's failure cannot be collapsed into or used to infer another target's result.
- Unless retained exact-0.146.0 evidence proves an ABA-safe individual turn-process capability,
  Beryl exposes none.
  `command/exec/terminate` belongs to a separate standalone, originating-connection process
  namespace. Experimental `thread/backgroundTerminals/terminate` reaches the turn-owned namespace
  but compares only a reusable numeric process id and cannot compare the provider item identity;
  a frozen id can therefore terminate a later process after ABA reuse. Neither method is admitted
  for individual hard stop, and a prior list read is not an atomic identity fence.
- Pinned `thread/backgroundTerminals/clean {threadId}` is optional coarse cleanup. Its empty
  response proves only core request acceptance, and its eventual scope is every unified-exec
  process then present in the loaded thread. It is not per-process or selected-turn completion
  evidence. Pinned child/subagent interruption is unavailable because its core interrupt is
  untargeted and Beryl cannot fence internally scheduled child successors.
- On the same loaded pinned session, cleanup enqueue completes before the empty response and one
  submission loop fully handles that queued op before receiving a later Beryl core operation.
  Callers may use the response as an ordering barrier for a later operation only while the exact
  session survives and their no-successor fence prevented an earlier submission. It remains
  non-evidence of cleanup completion in isolation.
- After experimental capability admission and local parameter validation, any JSON-RPC error from
  pinned coarse cleanup is authority-invalidating: its unstructured error cannot safely
  distinguish unloaded thread, capability drift, or core-channel failure. The exact session
  retires before another hard-target request.
- Capability probing proves only method shape and supported handle kind. It neither discovers live
  targets nor authorizes a production request. No hard-stop path may synthesize a handle from
  command text, process enumeration, working directory, names, or historical backend reads.
  Neither standalone command termination nor the experimental reusable-id turn-process family can
  stand in for an exact process-instance primitive.
- Coarse-cleanup capability admission combines the exact pinned release/source proof with the
  negotiated experimental API capability. Compatibility work never calls
  `thread/backgroundTerminals/clean` on a user thread merely to probe it, because that request is
  destructive and asynchronous.

## Connection Loss Recovery

- If a foreground backend connection or managed process is lost, the GUI keeps Beryl-home state, runtime/root records, selected Syndic thread, durable draft, and readable Syndic transcript state intact.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- On backend disconnect, the GUI presents a recovery path rather than silently switching backend process.
- Recovery actions may include relaunching a managed backend for the same runtime and resumed thread binding or closing the application instance.
- The GUI must not silently switch the user to a different backend process after disconnect.
- A lost connection or replaced process never recreates an interruption dispatch capability.
  Durable admitted or dispatch-claimed stop state converges through the CAS-live startup
  abandonment contract; runtime recovery does not resend it against either the old or a resumed
  target.

## Protocol Boundary

- Authentication, session storage, agent execution, subagents, configuration, skills, MCP, and other non-UI agent state remain backend-owned.
- Backend execution state, authentication, policy, sandbox behavior, tools, and live execution event streams remain backend-owned.
- Captured transcript history and selected transcript rendering source records are Syndic-owned after the CAS-live capture boundary accepts them.
- Turn execution stream inactivity is not itself backend failure.
- Request and probe timeouts apply to bounded JSON-RPC requests. Active turn streams may remain quiet until terminal events, protocol error, transport disconnect, or backend exit.
