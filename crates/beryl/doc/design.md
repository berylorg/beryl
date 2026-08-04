# Goals

Provide the Beryl executable entry point and composition root.

## Non-goals

- Owning reusable domain model types.
- Owning backend protocol parsing or process launch details.
- Owning reusable `gpui` window logic.

# Decisions

## Composition Root

- This crate remains the only binary crate in the workspace.
- This crate wires together `beryl-app`, `beryl-backend`, `beryl-home-store`, `beryl-model`, and the
  registered Beryl and Syndic storage domains.
- This crate owns process entry, bootstrap logging setup, and top-level startup failure propagation.
- This crate supplies service configuration for bounded pages, channels, worker counts, caches, and
  concurrency before content-dependent services start. It does not construct a universal process
  resource runtime or require unrelated services to share one accounting currency.
- This crate owns clap-based command-line parsing for executable startup options.
- This crate resolves normalized bootstrap configuration, including the selected Beryl home directory, opens the typed Beryl-home boundary, and passes either its validated domain services or its typed busy/open failure into the appropriate `beryl-app` startup surface.
- A successful ordinary startup first registers and validates only the bounded session domain, consumes its minimal restore-set discovery, and then registers the remaining required durable domains before ordinary windows admit state-dependent work.
- This crate does not expose Fjall, raw keyspaces, lock handles, or storage codecs to `beryl-app` while composing those services.
- This crate owns the diagnostic-target startup mode that launches Beryl as a controlled child process with an explicit isolated Beryl home directory and a stdio control channel.
- Diagnostic-target startup mode is the compatibility entry point for any Beryl executable selected by a supervisor diagnostic child launch, including a source-built executable that differs from the supervisor process executable.
- Diagnostic-target startup mode must reserve stdout for bounded protocol frames and route logs to stderr or files.
- Diagnostic-target startup mode must reject startup without an explicit Beryl home directory because implicit home fallback could collide with the supervisor instance.

## Scope Boundary

- Long-lived backend integration logic belongs in `beryl-backend`.
- Physical Beryl-home locking, database ownership, typed domain registration, and durability barriers belong in `beryl-home-store`.
- Syndic record schemas and typed conversation-history operations belong in `syndic-storage`.
- High-level application-shell behavior belongs in `beryl-app`.
- Shared pure-data types belong in `beryl-model`.
- Fixed-capacity pages and channels, bounded range source/sink contracts, structural backpressure,
  and content-free component telemetry belong in `beryl-stream`; semantic cancellation belongs to
  the service that owns each operation.
- Diagnostic-target command execution against live GUI state belongs in `beryl-app`; this crate only selects the startup mode and passes the normalized bootstrap configuration into that boundary.
