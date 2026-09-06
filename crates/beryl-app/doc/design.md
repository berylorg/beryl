# Goals

Own Beryl's sole GPUI desktop-shell composition boundary and expose typed, bounded orchestration
over Beryl-home, Syndic, transcript, and Codex App Server services.

Keep independent windows responsive while process-wide services preserve exact identity,
generation, custody, and replacement boundaries.

## Non-goals

- Owning process entry, CLI bootstrap, or configured-home selection.
- Owning feature-visible policy, cross-package durable or execution policy, physical storage,
  provider transport, transcript residency, or dependency-internal representation.
- Exposing raw Fjall or storage encoding, backend JSON, CAS historical transcripts, or GPUI-owned
  state across the package boundary.
- Providing compatibility models or adapters for removed product surfaces.

# Decisions

## Documentation Set

- [Shell and lifecycle](design-shell-lifecycle.md) is normative for the process service graph,
  window controllers, startup and activation inputs, typed home integration, and the app-owned
  contribution to same-home replacement.
- [Catalog and composer](design-catalog-and-composer.md) is normative for catalog projection and
  the range-backed composer, edit, history, publication, submission, activation, and title adapters.
- [Live control](design-live-control.md) is normative for approval, exact-stop, compaction, and
  lifecycle-continuation coordinator, driver, and custody surfaces.
- [Live projection and scheduling](design-live-projection-and-scheduling.md) is normative for
  runtime interest, connections, brokers, projections, leases, recovery establishment, accepted
  input scheduling, steering, execution, and outbound preparation.
- [Live capture](design-live-capture.md) is normative for outage buffering, ordered ingestion,
  observation and seal routing, bounded capture publication, audit, and repair adaptation.
- [Feature adapters](design-feature-adapters.md) is normative for package-local GUI, tool, audio,
  settings, theme, transcript, image, discussion, diagnostics, and status adapters.

These six supplements are part of this package design. Each is authoritative only for its stated
beryl-app role and cannot redefine feature-visible behavior, system policy, or a dependency's
internal contract.

## Package Responsibility

- `beryl-app` is the only Beryl package that composes the GPUI process shell, OS-window
  controllers, and feature mounts. It correlates typed services but creates no second durable
  authority.
- The package consumes app-neutral values and services and publishes typed versioned facts,
  bounded pages, command outcomes, and exact adapter inputs. Versioned facts never retain GPUI
  views, entities, windows, or renderer callbacks.
- Product behavior remains in feature authority. Cross-package storage, recovery, capture,
  execution, scheduling, and resource policy remains in system authority. Stored formats remain in
  their owning packages.

## Public Boundary

- Public commands and completions carry every applicable home, service, window, thread, draft,
  session, operation, runtime, root, connection, CAS, turn, item, revision, generation, and request
  identity. A stale or ambiguous result is rejected or reconciled by its typed owner and is never
  rebound by coincidence.
- Renderer-facing code consumes only prepared bounded app projections. Storage, backend, and
  provider workers return typed bounded values containing no GPUI state.
- The package never leaks raw Fjall handles or encodings, backend JSON, CAS history collections,
  dependency custody internals, or storage proof construction across its public surface.
- Every app-owned queue, worker set, page set, cache, retained projection, in-flight operation, and
  reconciliation scope has an explicit count, byte, or concurrency bound and a terminal release
  path. Logical durable content is paged or streamed rather than truncated to meet resident bounds.

## Cross-Cutting Guarantees

- Background work is fenced by exact durable identity, revision, and home/service generation.
  Cancellation, supersession, terminal failure, disposal, and same-home replacement cannot make an
  old completion current.
- No old-generation connection, broker, router lane, scheduler, projection, lease, loaded session,
  worker, operation custody, or request capability survives same-home service replacement.
- The GPUI thread performs no blocking filesystem, storage, process, transport, protocol, history,
  parsing, image, persistence, or model work.
- Correctness-sensitive commands preserve exact external-effect custody through acknowledgement,
  possible-dispatch, reconciliation, and terminal settlement. Indeterminate is custody, not a
  success, noncommit, or extra public terminal result.
- Package-owned state is bounded at every supported accumulation point. Completion, cancellation,
  failure, supersession, retirement, generation loss, and orderly shutdown release its exact
  reservations, permits, pages, handles, and waiters.

## Dependency Summary

- Pure identities and shared values come from `beryl-model`; typed home and domain services come
  from `beryl-home-store` and Beryl domain packages; normalized provider operations come from
  `beryl-backend`; transcript data is consumed only through transcript host/provider boundaries.
- Renderer-facing modules do not directly depend on `syndic-storage`, `beryl-home-store`,
  `beryl-backend`, Fjall, or raw app-server protocol types.
- `sha2` computes only the exact SHA-256 identity of the canonical serialized conversation-tool
  registry. The digest is durable profile correlation, not authorization or secrecy.
- Physical storage, Syndic records, backend protocol integration, transcript residency, and GUI
  rendering retain their distinct typed ownership; `beryl-app` only composes their public
  boundaries.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `external-side-effects/v2`

This rigor declaration governs this entry point and all six normative supplements. Supported-
envelope public contracts, exact external-effect custody, cancellation and replacement fences,
bounded retained state, and explicit failure outcomes require concrete verification and semantic
review; semantic loss is blocking.
