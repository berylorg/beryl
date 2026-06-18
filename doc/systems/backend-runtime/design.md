# Goals

Define Beryl's internal backend runtime system for launching, probing, connecting to, supervising, and recovering `codex app-server` runtime targets.

Preserve exact runtime, workspace-member, backend-process, thread, turn, auth, policy, sandbox, and protocol boundaries while allowing user-visible features to degrade predictably when a runtime target is unavailable.

## Non-goals

- Defining user-visible recovery copy, disabled states, or workspace workflows that belong in feature docs.
- Bundling, installing, replacing, or modifying Codex.
- Exposing operator-managed unauthenticated app-server listeners.
- Treating backend thread enumeration as the source of Beryl workspace identity.
- Owning Syndic durable transcript history, transcript projection, or transcript presentation policy.

# Decisions

## Runtime Ownership

- Beryl integrates with Codex through `codex app-server` as an out-of-process client.
- Beryl launches and owns managed backend processes in V1. It does not attach to already running app-server instances.
- Backend availability is tracked per runtime target, not globally.
- A backend-unavailable target disables backend-required operations for that target without detaching workspace members, changing default runtime, promoting another primary member, making the workspace unavailable, or requiring application exit.
- A backend-unavailable host-Windows target does not disable usable WSL targets, and a backend-unavailable WSL distro does not disable host-Windows or other WSL distros.

## Launch And Listener Security

- Host-Windows launch uses the `codex` executable from the user's `PATH`.
- WSL launch uses `wsl.exe`, targets the selected distro, sets the requested working directory, and runs `codex app-server` inside the distro.
- A Beryl-owned app-server listens on an authenticated loopback WebSocket endpoint chosen by Beryl.
- Beryl generates a high-entropy capability token per managed launch, stores it only in a per-run local token file and memory, passes the token file to app-server auth configuration, uses the token in WebSocket handshakes, and removes the token file when the server exits.
- Managed listeners bind only to loopback addresses and must not expose unauthenticated non-loopback endpoints.
- Cross-boundary communication uses the app-server contract rather than direct access to Codex storage or process memory.

## Backend Lifecycle

- Launching a backend through a local child process, including through `wsl.exe`, is a supported lifecycle mode.
- The GUI owns every backend child process it launches and terminates those processes when no longer needed or when the GUI exits.
- Managed termination covers the supervised runtime boundary, including descendants that would otherwise outlive the immediate child.
- Host-Windows launches are supervised as Windows process trees.
- WSL launches create a Beryl-owned cleanup boundary inside the selected distro.
- Normal window close and in-app quit explicitly shut down active managed backend processes before process exit. Destructors are fallback only.
- Backend process lifetime is separate from backend client connection lifetime.
- Dropping one client connection must not terminate a managed app-server still needed by other work.
- A GUI instance may manage backend processes for multiple workspace runtime targets as needed, but uses at most one active managed app-server process for a given runtime target.
- Foreground turns, thread activation, inventory refresh, title generation, status operations, and lazy maintenance use independent backend client connections when sharing one connection would delay foreground streaming or completion.

## Capability Probing

- Beryl probes backend compatibility when a managed backend is launched or explicitly probed for a runtime target.
- Compatibility probing validates the required app-server contract for the current Beryl target version and capability set.
- Required capabilities include exact thread resume by id, metadata-only resume, live turn event streaming suitable for Syndic capture, paginated thread summary listing, thread summary filters for member inventory, model listing, cwd-scoped config reads, ordered text and local-image user input on turn start or steering, developer-instructions payloads, filesystem reads for runtime-readable transcript media, active-turn steering by expected turn id, active-turn interruption, thread compaction, dynamic tool registration, and reverse dynamic tool calls.
- CAS historical turn reads through `thread/turns/list` and full-history `thread/read` are obsolete under the CAS-live Syndic transcript rework. Backend compatibility probing must not require them, and live backend code must not retain them as a compatibility surface.
- Branch actions depend on app-server fork and rollback primitives. When missing, branch actions are disabled rather than emulated.
- Edit actions depend on app-server rollback and turn-start primitives plus exact rollback-scope proof. When missing or unprovable, edit actions are disabled rather than emulated.
- Hard-stop backend primitives are probed separately. Missing hard-stop support disables only affected hard-stop escalation targets and must not disable soft interruption.

## Connection Loss Recovery

- If the foreground backend connection or managed process is lost, the GUI keeps workspace state, semantic graph state, checklist selection, runtime state, member state, selected transcript selection, and GUI-local drafts intact.
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
