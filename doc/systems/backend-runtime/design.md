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
- A Beryl-owned app-server listens on an authenticated loopback WebSocket endpoint chosen by Beryl.
- Beryl generates a high-entropy capability token per managed launch, stores it only in a per-run local token file and memory, passes the token file to app-server auth configuration, uses the token in WebSocket handshakes, and removes the token file when the server exits.
- Managed listeners bind only to loopback addresses and must not expose unauthenticated non-loopback endpoints.
- Cross-boundary communication uses the app-server contract rather than direct access to Codex storage or process memory.
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
- Required capabilities include exact thread resume by id for approved live execution/control, live turn event streaming suitable for Syndic capture, model listing, cwd-scoped config reads, ordered text and local-image user input on turn start or steering, developer-instructions payloads, filesystem reads for runtime-readable transcript media, active-turn steering by expected turn id, active-turn interruption, thread compaction, dynamic tool registration, and reverse dynamic tool calls.
- Required capabilities include exact native CAS continuation and fork plus stable 0.144.1 `thread/inject_items` with the recovery semantics defined by `doc/systems/cas-live-syndic-transcript/design.md`.
- Foreground-stream compatibility is release-scoped. For pinned CAS 0.144.1 it requires an
  uninterrupted full-notification subscription, serial item-before-terminal FIFO consumption for
  normally finishing ordinary turns, a
  closed disposition for every public pinned item variant, typed item-delta discrimination, and
  fail-closed loss handling because reconnect, resume, late subscription, and process restart do
  not replay notifications. A CAS-version change must refresh the retained source and installed
  runtime proofs before admission can rely on equivalent semantics.
- Hosted Responses image generation is not a required or supported CAS 0.144.1 producer capability:
  the pinned client cannot send the native `image_generation` tool declaration. Standalone
  `image_gen.imagegen` is the admitted image-generation path and is probed and normalized
  separately. Parser tolerance for an unsolicited hosted item from a nonconforming custom provider
  does not admit that provider behavior.
- Branch execution additionally requires the exact one-time selected-context projection proven by `doc/systems/cas-live-syndic-transcript/design.md`; schema presence without accepted-limit and trust-semantics proof is insufficient.
- CAS thread-list, CAS historical turn reads through `thread/turns/list`, and full-history `thread/read` are not Beryl catalog or transcript capabilities. Compatibility probing must not require them, and live backend code must not retain them as a fallback surface.
- Branch-discussion creation is Syndic-native and performs no CAS work before first user submission. Its later execution prefers exact CAS-native inherited parent context and uses fresh recovery injection only when that lineage is unavailable or unprovable.
- Edit actions depend on app-server rollback and turn-start primitives plus exact rollback-scope proof. When missing or unprovable, edit actions are disabled rather than emulated.
- Hard-stop backend primitives are probed separately. Missing hard-stop support disables only affected hard-stop escalation targets and must not disable soft interruption.

## Connection Loss Recovery

- If a foreground backend connection or managed process is lost, the GUI keeps Beryl-home state, runtime/root records, selected Syndic thread, durable draft, and readable Syndic transcript state intact.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- On backend disconnect, the GUI presents a recovery path rather than silently switching backend process.
- Recovery actions may include relaunching a managed backend for the same runtime and resumed thread binding or closing the application instance.
- The GUI must not silently switch the user to a different backend process after disconnect.

## Protocol Boundary

- Authentication, session storage, agent execution, subagents, configuration, skills, MCP, and other non-UI agent state remain backend-owned.
- Backend execution state, authentication, policy, sandbox behavior, tools, and live execution event streams remain backend-owned.
- Captured transcript history and selected transcript rendering source records are Syndic-owned after the CAS-live capture boundary accepts them.
- Turn execution stream inactivity is not itself backend failure.
- Request and probe timeouts apply to bounded JSON-RPC requests. Active turn streams may remain quiet until terminal events, protocol error, transport disconnect, or backend exit.
