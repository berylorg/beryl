# Goals

Keep Beryl workspaces usable when runtime targets or backend connections are unavailable, while preserving exact backend ownership, security, and thread/runtime bindings.

## Non-goals

- Bundling, installing, or replacing Codex.
- Exposing operator-managed unauthenticated app-server listeners.
- Silently switching users to a different backend process, runtime target, workspace member, or thread after failure.
- Treating backend thread enumeration as the source of workspace identity.

# Decisions

## Managed Backend Availability

- Beryl integrates with Codex through `codex app-server` as an out-of-process client.
- Beryl launches and owns managed backend processes in V1. It does not attach to already running app-server instances.
- Host-Windows launch uses the `codex` executable from the user's `PATH`.
- WSL launch uses `wsl.exe`, targets the selected distro, sets the requested working directory, and runs `codex app-server` inside the distro.
- A Beryl-owned app-server listens on an authenticated loopback WebSocket endpoint chosen by Beryl.
- Beryl generates a high-entropy capability token per managed launch, stores it only in a per-run local token file and memory, passes the token file to app-server auth configuration, uses the token in WebSocket handshakes, and removes the token file when the server exits.
- Managed listeners bind only to loopback addresses and must not expose unauthenticated non-loopback endpoints.
- Backend availability is tracked per runtime target, not globally.
- Missing `codex`, managed process spawn failure, probe failure, or incompatible required capability marks only that target backend-unavailable.
- Backend-unavailable targets disable backend-required operations for that target until successful retry or runtime configuration change.
- Backend-unavailable state does not detach workspace members, change default runtime, promote another primary member, make the workspace unavailable, or require application exit.
- A backend-unavailable host-Windows target does not disable usable WSL targets, and a backend-unavailable WSL distro does not disable host-Windows or other WSL distros.

## Workspace Behavior During Backend Failure

- Opening the workspace shell requires only GUI-owned workspace state.
- Successful startup does not require backend launch, compatibility probing, or backend thread enumeration.
- Reopening one persisted active conversation thread uses exact backend validation by its registered id and binding. It does not depend on completing or exhausting backend thread enumeration.
- Backend thread enumeration used for startup candidates or selector inventory is member-scoped and bounded end to end. Exhaustion, truncation, or failure of that background projection must not erase a separately validated exact active-thread recovery result.
- If the current primary runtime target cannot launch or probe during startup, Beryl still opens the workspace, keeps workspace/member management available, and disables conversation operations for that target.
- Workspace selection, workspace picker interaction, default-runtime selection, member attachment, member detachment, and primary-member selection remain available while the primary runtime target is backend-unavailable.
- Composer submission, new-thread creation, existing-thread activation, thread selector activation, inventory refresh, title generation, backend-derived model/reasoning status, backend-derived context status, context compaction, and turn-control interactions are disabled for the affected backend-unavailable target.
- Backend-unavailable user-facing states identify the affected runtime target.

## Backend Lifecycle

- Launching a backend through a local child process, including through `wsl.exe`, is a supported lifecycle mode.
- The GUI owns every backend child process it launches and terminates those processes when no longer needed or when the GUI exits.
- Managed termination covers the supervised runtime boundary, including descendants that would otherwise outlive the immediate child.
- Host-Windows launches are supervised as Windows process trees. WSL launches create a Beryl-owned cleanup boundary inside the selected distro.
- Normal window close and in-app quit explicitly shut down active managed backend processes before process exit. Destructors are fallback only.
- Backend process lifetime is separate from backend client connection lifetime. Dropping one client connection must not terminate a managed app-server still needed by other work.
- A GUI instance may manage backend processes for multiple workspace runtime targets as needed, but uses at most one active managed app-server process for a given runtime target.
- Foreground turns, thread activation, inventory refresh, title generation, status operations, and lazy maintenance use independent backend client connections when sharing one connection would delay foreground streaming or completion.

## Required Capabilities And Disabled Paths

- Beryl probes backend compatibility when a managed backend is launched or explicitly probed for a runtime target.
- Required capabilities include exact thread resume by id, metadata-only resume, paginated turn history reads, paginated thread summary listing, thread summary filters for member inventory, model listing, cwd-scoped config reads, ordered text and local-image user input on turn start/steering, developer-instructions payloads, filesystem reads for runtime-readable transcript media, active-turn steering by expected turn id, active-turn interruption, thread compaction, dynamic tool registration, and reverse dynamic tool calls.
- Branch actions depend on app-server fork and rollback primitives. When missing, branch actions are disabled rather than emulated.
- Edit actions depend on app-server rollback and turn-start primitives plus exact rollback-scope proof. When missing or unprovable, edit actions are disabled rather than emulated.
- Hard-stop backend primitives are probed separately. Missing hard-stop support disables only affected hard-stop escalation targets and must not disable soft interruption.
- Operations that target incompatible or unavailable backends fail or present localized recovery for that target and must not silently switch runtime target, workspace member, backend process, or thread.

## Connection Loss Recovery

- If the foreground backend connection or managed process is lost, the GUI keeps the current workspace, semantic graph state, checklist selection, runtime state, member state, active transcript selection, and GUI-local drafts intact.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- On backend disconnect, the GUI presents a blocking recovery path rather than silently switching backend process.
- Recovery actions may include relaunching a managed backend for the same runtime and resumed thread binding or closing the application instance.
- The GUI must not silently switch the user to a different backend process after disconnect.

## Protocol Boundary

- Cross-boundary communication uses the app-server contract rather than direct access to Codex storage or process memory.
- Authentication, session storage, agent execution, subagents, configuration, skills, MCP, and other non-UI agent state remain backend-owned.
- Backend conversation thread contents and execution event streams remain backend-owned.
- Turn execution stream inactivity is not itself backend failure. Request/probe timeouts apply to bounded JSON-RPC requests; active turn streams may remain quiet until terminal events, protocol error, transport disconnect, or backend exit.
