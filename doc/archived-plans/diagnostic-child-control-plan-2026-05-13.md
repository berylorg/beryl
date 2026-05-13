# Scope

Implement isolated Beryl diagnostic-control tooling so the AI can inspect and operate on a child Beryl instance without mutating the supervisor Beryl session that is hosting the active turn.

The previous memory-investigation plan has been archived at `doc/archived-plans/memory-investigation-plan-2026-05-13.md`. That investigation remains useful background, but the active implementation plan is now the diagnostic child-control surface.

Target architecture:

- Keep the existing read-only supervisor diagnostics that report the currently running supervisor Beryl process and GUI state.
- Remove the local supervisor GUI-modifying tools from the app-server dynamic tool registry.
- Add a `beryl.diagnostic` dynamic-tool namespace implemented by the supervisor Beryl.
- Let the supervisor launch exactly one diagnostic child Beryl process with an isolated Beryl home directory.
- Let the supervisor send bounded command requests to the child over a narrow stdio JSON protocol.
- Let the child execute UI state reads and GUI-control commands against its own GUI shell, reusing the existing shell operations where possible.
- Keep logs off protocol stdout so child protocol responses remain parseable.

Design constraints:

- The diagnostic child must never use the supervisor's active Beryl home.
- The child-control protocol must not expose unbounded transcript, image, log, or process payloads.
- The child-control path must remain independent of the child's app-server turn lifecycle, so child GUI commands can run even when the child has an active turn.
- Local supervisor read-only diagnostics remain observation tools and must not mutate durable state.
- Supervisor diagnostic lifecycle tools may start, stop, or query the diagnostic child, but must not mutate the supervisor's transcript, semantic graph, workspace persistence, or settings.
- GUI-modifying diagnostic commands target only the diagnostic child.

Edge-case checklist:

- Starting twice must be idempotent or return a clear already-running response with the existing child identity.
- Stop must handle missing child, crashed child, pending requests, and bounded shutdown timeout.
- The child home must be generated or supplied as an explicit isolated path and must reject the supervisor home path.
- Child stdout must carry only protocol frames; child logs must use stderr or files.
- Protocol requests and responses must enforce maximum frame size, timeout, request id correlation, and clear parse-error behavior.
- GUI-control commands sent to the child must preserve existing availability checks inside the child shell.
- `switch_thread` must still reject ambiguous or unavailable thread ids in the child.
- `scroll_transcript` must stay bounded by repeat limits and return a post-command child UI snapshot.
- `close_popups` must close only transient child UI popups and must not hide child graph overlay state unless a future design adds that command explicitly.
- Child crash or protocol EOF must clear supervisor lifecycle state and report a diagnostic error without crashing the supervisor.
- Tests must verify that old local supervisor GUI-control tools are no longer registered, while read-only supervisor diagnostics remain registered.
- Verification must avoid launching a second Beryl against the active supervisor home.

# Phase 1: Establish diagnostic-control contract (finished)

Update design documentation for the isolated diagnostic child architecture and preserve the archived memory investigation plan.

Work items:

- Update root `doc/design.md` to replace local GUI-control dynamic tools with isolated diagnostic child-control tools.
- Update `crates/beryl/doc/design.md` for the executable-owned diagnostic-target CLI mode and stdio protocol bootstrap.
- Update `crates/beryl-app/doc/design.md` for app-shell ownership of child-control operations, read-only supervisor diagnostics, and reusable child GUI-control handlers.
- Ensure the docs state that supervisor-local GUI mutation is not part of the diagnostic dynamic-tool surface.

Verification cases:

- Confirm no design doc tells diagnostic child tools to use the supervisor's GUI state.
- Confirm read-only supervisor diagnostics are still allowed.
- Confirm child-control commands are bounded and target-isolated.

Phase 1 outcome, 2026-05-13:

- Archived the previous memory investigation plan at `doc/archived-plans/memory-investigation-plan-2026-05-13.md`.
- Updated root `doc/design.md` to define supervisor read-only diagnostics separately from `beryl.diagnostic` child-control tools.
- Updated `crates/beryl/doc/design.md` to define the executable-owned diagnostic-target stdio startup mode and explicit isolated-home requirement.
- Updated `crates/beryl-app/doc/design.md` to remove local supervisor GUI-control tools from the target app-server dynamic-tool surface while preserving child-targeted GUI-control behavior through diagnostic child control.
- Verified the design contract keeps supervisor diagnostics read-only, requires child home isolation, and routes GUI-changing commands only to the diagnostic child.

# Phase 2: Remove local supervisor GUI-control tools and preserve reusable operations (finished)

Remove `read_ui_state`, `switch_thread`, `scroll_transcript`, and `close_popups` from the supervisor `beryl` dynamic-tool namespace while preserving their command/result structures and shell logic for child diagnostic use.

Work items:

- Remove local GUI-control tool specs from the ordinary dynamic-tool registry.
- Remove local GUI-control routing from active-turn shell bridge paths.
- Refactor GUI-control parsing/result structs as internal diagnostic child command DTOs where useful.
- Keep read-only diagnostic tool registration and handling intact.
- Update or replace tests that expected local GUI-control tools to be registered.

Verification cases:

- `beryl.read_*` supervisor diagnostics remain registered.
- `beryl.switch_thread`, `beryl.scroll_transcript`, and `beryl.close_popups` are not advertised as local supervisor tools.
- Existing GUI-control operation logic remains reachable for child shell control.

Phase 2 outcome, 2026-05-13:

- Removed the local supervisor GUI-control spec registration from the ordinary `beryl` dynamic-tool registry.
- Removed local GUI-control forwarding from the active-turn shell bridge, so `read_ui_state`, `switch_thread`, `scroll_transcript`, and `close_popups` now resolve as unsupported local supervisor dynamic tools.
- Removed public `beryl-app` exports for the local GUI-control tool names/spec helper while keeping the parser, result DTOs, and shell operations as private reusable implementation code for later diagnostic-child control.
- Updated registration and turn-worker tests to verify read-only supervisor diagnostics remain registered, local GUI-control tools are absent from the registry, and local GUI-control calls are not forwarded through the supervisor shell bridge.
- Verification passed: `cargo fmt --check`; `cargo nextest run -p beryl-app --test workspace_graph_dynamic_tools --test gui_control_dynamic_tools --test turn_worker --test turn_worker_graph_dynamic` (53/53 tests).

# Phase 3: Add diagnostic child process lifecycle and stdio protocol (finished)

Implement the supervisor-owned diagnostic child process lifecycle and the child-owned stdio command loop.

Work items:

- Add CLI support for launching a Beryl process as a diagnostic target with `--diagnostic-target-stdio` and an explicit isolated Beryl home.
- Add a bounded stdio JSON request/response protocol for child diagnostic commands.
- Add supervisor lifecycle state for starting, stopping, crash detection, request timeouts, and child process identity.
- Ensure child process stdout is reserved for protocol frames and logs use stderr or files.
- Reuse existing process supervision primitives where they fit cleanly; otherwise add a small diagnostic-process supervisor with equivalent cleanup semantics.

Verification cases:

- Starting without an isolated home is rejected.
- Starting with the supervisor home is rejected.
- EOF, malformed JSON, oversized frames, and timeout paths return bounded errors.
- Stopping a running child terminates the child process tree or reports a bounded failure.

Phase 3 outcome, 2026-05-13:

- Added `--diagnostic-target-stdio`, requiring an explicit `--beryl-home-dir`, and routed diagnostic-target startup before normal GUI startup.
- Reserved child stdout for newline-delimited bounded JSON protocol frames and moved executable tracing output to stderr.
- Added child-side stdio request handling that runs independently of the active-turn shell bridge and dispatches diagnostic reads plus reused GUI-control shell operations against the child shell.
- Added supervisor-side diagnostic child lifecycle state for start, stop, status, bounded request/response correlation, timeout handling, stderr draining, and Windows process-tree cleanup.
- Added same-home rejection so a diagnostic child cannot be launched against the supervisor Beryl home.
- Added protocol and lifecycle tests for CLI parsing, malformed JSON, oversized frames, bounded response errors, idempotent stop, and supervisor-home rejection.
- Verification passed: `cargo fmt --check`; `cargo check -p beryl-app -p beryl`; `cargo nextest run -p beryl-app --test diagnostic_child_protocol --test diagnostic_child_supervisor --test gui_control_dynamic_tools --test diagnostic_dynamic_tools --test workspace_graph_dynamic_tools --test turn_worker` (66/66 tests); `cargo nextest run -p beryl-app --test turn_worker_graph_dynamic` (5/5 tests); `CARGO_TARGET_DIR=target\diagnostic-nextest cargo nextest run -p beryl --test cli` (12/12 tests); `git diff --check`.
- The alternate `CARGO_TARGET_DIR` was used for the `beryl` CLI tests because the operator's active Beryl process locks `target/debug/beryl.exe` on Windows.

# Phase 4: Add `beryl.diagnostic` dynamic tools (finished)

Expose child diagnostic lifecycle, read-only snapshots, and child GUI-control commands through a separate app-server dynamic-tool namespace implemented by the supervisor.

Work items:

- Add `beryl.diagnostic` specs for `start`, `stop`, `status`, `read_process`, `read_memory`, `read_ui_state`, `read_retained_state`, `read_visible_media`, `read_media_events`, `switch_workspace`, `switch_thread`, `scroll_transcript`, and `close_popups`.
- Route these tools through the supervisor shell to the child lifecycle or child stdio protocol as appropriate.
- Shape success and failure responses consistently with existing dynamic-tool JSON text payloads.
- Keep command schemas explicit and bounded.

Verification cases:

- Calling child read/control tools before `start` returns a clear not-running error.
- `start` returns child pid, home, and lifecycle state without exposing unbounded paths.
- Child GUI-control commands return child UI snapshots, not supervisor UI snapshots.
- The old local supervisor GUI-control namespace remains absent.

Phase 4 outcome, 2026-05-13:

- Added the separate `beryl.diagnostic` dynamic-tool namespace with child lifecycle tools `start`, `stop`, and `status`; child read tools `read_process`, `read_memory`, `read_ui_state`, `read_retained_state`, `read_visible_media`, and `read_media_events`; and child GUI-control tools `switch_workspace`, `switch_thread`, `scroll_transcript`, and `close_popups`.
- Registered `beryl.diagnostic` tools alongside the existing `beryl` graph, lifecycle, and read-only supervisor diagnostics while keeping local supervisor GUI-control tools absent from the ordinary `beryl` namespace.
- Routed `beryl.diagnostic` calls through the live shell dynamic-tool bridge into a background worker that owns supervisor-side child lifecycle and stdio requests through a mutex-protected diagnostic child supervisor, so child process I/O does not run on the GPUI thread.
- Added child protocol and shell handling for `switch_workspace`, reusing the existing workspace activation worker path and returning `pending`, `already_selected`, or bounded rejection errors rather than waiting indefinitely for backend readiness.
- Kept child GUI-control parsing bounded and shared with child target protocol handling; child read/control calls before `start` now return `diagnostic_child_not_running`.
- Verification passed: `cargo fmt --check`; `cargo check -p beryl-app -p beryl`; `cargo nextest run -p beryl-app --test diagnostic_child_dynamic_tools --test diagnostic_child_protocol --test gui_control_dynamic_tools --test workspace_graph_dynamic_tools --test turn_worker --test diagnostic_dynamic_tools --test diagnostic_child_supervisor` (73/73 tests); `cargo nextest run -p beryl-app --test turn_worker_graph_dynamic --test workspace_picker` (38/38 tests); `git diff --check`.
- Verification emitted only the pre-existing GPUI unreachable-expression warning in `zed-fork/crates/gpui/src/elements/surface.rs` plus expected dead-code warnings from path-included test modules.

# Phase 5: Verify and review (finished)

Run focused tests and review the completed implementation.

Work items:

- Add/update unit and integration tests for CLI parsing, tool registration, response parsing, protocol framing, lifecycle state, and removed local GUI-control tools.
- Run `cargo nextest` for affected workspace projects.
- When all phases are finished, request reviewer subagent review of code and doc changes and address any findings through an updated plan before finalizing.

Verification cases:

- Tests pass with `cargo nextest`.
- No diagnostic child test uses the operator's active Beryl home.
- Reviewer findings are either fixed or explicitly planned before completion.

Reviewer findings to fix, 2026-05-13:

- The supervisor shell dynamic-tool timeout must not report `beryl.diagnostic.stop` as unavailable while the background stop worker is still inside its documented bounded shutdown window.
- Diagnostic child process ownership must not be lost if child startup fails after OS process spawn, and `stop` must not clear lifecycle state unless child cleanup is complete or proven unnecessary.
- Add targeted tests for timeout budget constants and diagnostic child supervisor cleanup/ownership behavior.

Phase 5 outcome, 2026-05-13:

- Ran final verification for formatting, build, affected `beryl-app` tests, `beryl` CLI tests, and diff whitespace hygiene.
- Reviewer found two blocking lifecycle issues: the shell bridge timeout was too short for `beryl.diagnostic.stop`, and diagnostic child process ownership could be lost on startup or stop failure paths.
- Fixed `beryl.diagnostic.stop` to use an extended shell response timeout that exceeds the diagnostic child stop budget.
- Added a spawned-child guard so child startup failures after `Command::spawn()` clean up the unclaimed child process.
- Changed diagnostic child `stop` so shutdown errors restore lifecycle state for later status or retry instead of reporting not-running while ownership is uncertain.
- Added targeted tests for the stop timeout budget, startup cleanup guard, failed-stop ownership restoration, and extended shell timeout routing.
- Reviewer re-review reported no blocking findings.
- Verification passed: `cargo fmt --check`; `git diff --check`; `cargo check -p beryl-app -p beryl`; `cargo nextest run -p beryl-app --test diagnostic_child_dynamic_tools --test diagnostic_child_protocol --test diagnostic_child_supervisor --test diagnostic_dynamic_tools --test gui_control_dynamic_tools --test workspace_graph_dynamic_tools --test turn_worker --test turn_worker_graph_dynamic --test workspace_picker` (115/115 tests); `$env:CARGO_TARGET_DIR='target\diagnostic-nextest'; cargo nextest run -p beryl --test cli` (12/12 tests).
