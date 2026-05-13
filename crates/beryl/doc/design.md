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

## Scope Boundary

- Long-lived backend integration logic belongs in `beryl-backend`.
- High-level application-shell behavior belongs in `beryl-app`.
- Shared pure-data types belong in `beryl-model`.
- Diagnostic-target command execution against live GUI state belongs in `beryl-app`; this crate only selects the startup mode and passes the normalized bootstrap configuration into that boundary.
