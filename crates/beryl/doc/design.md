# Goals

Provide the Beryl executable entry point and composition root.

## Non-goals

- Owning reusable domain model types.
- Owning backend protocol parsing or process launch details.
- Owning reusable `gpui` window logic.

# Decisions

## Composition Root

- This crate remains the only binary crate in the workspace.
- This crate wires together `beryl-app`, `beryl-backend`, and `beryl-model`.
- This crate owns process entry, bootstrap logging setup, and top-level startup failure propagation.
- This crate owns clap-based command-line parsing for executable startup options.
- This crate forwards normalized bootstrap configuration, including the selected Beryl home directory, into `beryl-app`.
- This crate owns the diagnostic-target startup mode that launches Beryl as a controlled child process with an explicit isolated Beryl home directory and a stdio control channel.
- Diagnostic-target startup mode is the compatibility entry point for any Beryl executable selected by a supervisor diagnostic child launch, including a source-built executable that differs from the supervisor process executable.
- Diagnostic-target startup mode must reserve stdout for bounded protocol frames and route logs to stderr or files.
- Diagnostic-target startup mode must reject startup without an explicit Beryl home directory because implicit home fallback could collide with the supervisor instance.
- An internal hidden acceptance-only startup-gate flag additionally requires diagnostic-target mode. When present, process entry reads and validates one fixed bounded stdin gate frame immediately after CLI parsing, writes and flushes one fixed pre-protocol ready frame, and only then performs tracing, workspace resolution, application bootstrap, or diagnostic protocol startup. EOF, invalid, and oversized gate input fail closed.

## Diagnostic Acceptance Entry Point

- This crate implements the command-line and request-plan responsibilities assigned to `beryl` by `doc/systems/diagnostic-acceptance/design.md`.
- The `beryl-acceptance` executable requires explicit frozen-executable, isolated-home, evidence, run-identity, and request-plan inputs plus bounded limits and a validated launch configuration. Fresh-workspace remains the default launch mode and requires an execution workspace. Existing-home recovery requires the isolated home to exist and rejects an execution-workspace argument so the child starts without `--host-path`.
- The acceptance executable supports host Windows only and fails before launching Beryl on other hosts.
- The executable configures one separate bounded recovery-cleanup budget. An owner-bearing startup failure consumes it before returning the bounded launch error; after successful startup, an indeterminate initial terminal cleanup may instead consume it once before evidence publication. Those paths are mutually exclusive.
- Its JSON request plan is versioned, byte- and count-bounded, and rejects unknown fields. Before invoking the acceptance-session starter, this crate submits every raw entry to the `beryl-app` diagnostic-operation compiler with the configured request timeout, and launches only after the entire sequential plan has compiled successfully. CLI integration proves launch-none rejection for public plan-file and semantic payload violations; exact protocol-frame boundaries remain compiler/protocol-layer evidence because those public maxima cannot currently produce an otherwise-valid oversized frame.
- The compiled plan may contain supported one-shot diagnostic protocol requests and the canonical bounded `wait_for_state` supervisor/session operation. This crate does not implement command vocabulary, argument semantics, frame sizing, wait predicates, or polling behavior itself.
- The configured request count applies to logical plan entries. The `beryl-app` compiler additionally rejects a plan whose worst-case expanded protocol-request count exceeds its Beryl-owned maximum or whose summed worst-case operation budgets exceed the configured total runtime.
- This crate consumes the must-use terminal finish outcome, reports cleanup and publication failures independently or together, and explicitly releases any post-publication retained owner through its non-waiting fail-safe operation. It does not parse or implement the diagnostic-child wire protocol, own child-process cleanup, or define feature-specific live-acceptance assertions.

## Scope Boundary

- Long-lived backend integration logic belongs in `beryl-backend`.
- High-level application-shell behavior belongs in `beryl-app`.
- Shared pure-data types belong in `beryl-model`.
- Diagnostic-target command execution against live GUI state belongs in `beryl-app`; this crate only selects the startup mode and passes the normalized bootstrap configuration into that boundary.
