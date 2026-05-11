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

## Scope Boundary

- Long-lived backend integration logic belongs in `beryl-backend`.
- High-level application-shell behavior belongs in `beryl-app`.
- Shared pure-data types belong in `beryl-model`.
