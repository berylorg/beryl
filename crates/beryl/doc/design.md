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
- This crate resolves normalized bootstrap configuration, including the selected Beryl home directory,
  and opens an unpublished private typed Beryl-home candidate.
- Within that one candidate, this crate registers or reacquires the complete required typed Beryl and
  Syndic domain stack, constructs its required dependent services, and atomically publishes the
  complete healthy stack before any minimal session or restore discovery. Bootstrap has no
  partial-registration, partial-typed-handle, or session-only registration path.
- This crate passes either the published complete typed stack or its typed busy/open failure into
  the appropriate `beryl-app` startup surface.
- This crate does not expose Fjall, raw keyspaces, lock handles, or storage codecs to `beryl-app` while composing those services.
- This crate owns the diagnostic-target startup mode that launches Beryl as a controlled child process with an explicit isolated Beryl home directory and a stdio control channel.
- Diagnostic-target startup mode is the compatibility entry point for any Beryl executable selected by a supervisor diagnostic child launch, including a source-built executable that differs from the supervisor process executable.
- Diagnostic-target startup mode must reserve stdout for bounded protocol frames and route logs to stderr or files.
- Diagnostic-target startup mode must reject startup without an explicit Beryl home directory because implicit home fallback could collide with the supervisor instance.
- The target-mode control endpoint answers the bounded startup handshake with the exact diagnostic
  protocol name and version. The process is not a compatible started target until that matching
  handshake succeeds; malformed, absent, or mismatched handshake output is startup failure rather
  than permission to continue with an unverified command channel.
- End-of-file on the target's control input or loss of its response output closes the control loop,
  requests orderly app-shell shutdown, and accepts no later command. Target mode never falls back to
  ordinary startup, reconnects a replacement channel, or transfers live child state after that
  disconnect.
- Diagnostic-target process exit is terminal for that exact child instance and every outstanding
  request on its channel. Further control requires a newly launched target and a fresh successful
  handshake; an exit status or channel disconnect cannot be reused as live command authority.

## Scope Boundary

- The executable facade accepts normalized bootstrap or diagnostic-target startup inputs and returns
  only process startup success or a top-level typed failure. It exposes no reusable domain,
  storage, backend, stream, or GUI service API of its own.
- Composition consumes the public boundaries of
  [`beryl-app`](../../beryl-app/doc/design.md),
  [`beryl-backend`](../../beryl-backend/doc/design.md),
  [`beryl-home-store`](../../beryl-home-store/doc/design.md),
  [`beryl-model`](../../beryl-model/doc/design.md),
  [`beryl-stream`](../../beryl-stream/doc/design.md), and
  [`syndic-storage`](../../syndic-storage/doc/design.md) without redefining their responsibilities.
- For diagnostic-target startup, this crate selects the mode and forwards the normalized bootstrap
  configuration and bounded control channel into `beryl-app`; it does not execute live GUI commands
  itself.
