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

## Executable Bootstrap Facade

`bootstrap::Configuration` accepts an absolute configured home, the composition root's typed
home opener and diagnostic-target selection. It fixes bounded service/window inputs before
ordinary work. `bootstrap::run` owns the GPUI process lifetime through zero-window startup,
Retry, complete running-owner handoff and orderly shutdown. The opener returns one complete
private publication candidate with its typed domains, Busy, or failure retaining unsuccessful
close custody; it never publishes a partial graph. The private owners and their guarantees remain
those in [Shell And Lifecycle](design-shell-lifecycle.md).

Executable composition can attach one immutable optional native WSL supervisor descriptor through
`Configuration::with_wsl_supervisor_artifact`. It is preserved through service preparation and
recovery; absence makes managed WSL launch unavailable without disabling Host runtimes.

Consuming projection-service close returns `ProjectionConnectionServiceCloseFailure` when
disposal remains unsettled. That failure owns the original service and error. `retry` repeats
supported runtime retirement on that same service before component teardown begins. Terminal
connection, worker or home failures remain owned failures; retry cannot turn started or partial
teardown into positive closure. A later component failure is retained alongside the original
error and exposed as auxiliary failure custody. Transferring the parts transfers the same custody.
Callers retain it until complete disposal and cannot publish a replacement over unresolved cleanup.

The diagnostic endpoint keeps its bounded protocol and terminal channel-loss shutdown intent
across that handoff. It reports supported lifecycle facts and explicit unavailable feature
bindings; diagnostic commands do not gain authority to bypass ordinary lifecycle gates.

## Documentation Set

- [Runtime and root admission](design-runtime-root-admission.md) is normative for selected-path
  validation, atomic onboarding, outcome custody and capability retirement before shell mounting.
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
- Its isolated crash-report module exposes reporter startup, fatal-hook installation and the
  report-only GPUI entry required by the [crash-reporting system](../../../doc/systems/crash-reporting/design.md).
  That entry constructs no ordinary service graph or Beryl-home state.
- The process service graph owns admitted thread-execution sessions, scheduler flights, pending
  request routing, runtime interest, and coordinated shutdown. Main-window controllers own view
  claims, editors, and subscriptions; removing a controller cannot revoke required execution.
- The package consumes app-neutral values and services and publishes typed versioned facts,
  bounded pages, command outcomes, and exact adapter inputs. Versioned facts never retain GPUI
  views, entities, windows, or renderer callbacks.
- Product behavior remains in feature authority. Cross-package storage, recovery, capture,
  execution, scheduling, and resource policy remains in system authority. Stored formats remain in
  their owning packages.

## Public Boundary

- `crash_reporting::install` takes the current executable path and installs fatal handling before
  ordinary startup; setup failure retains abort-only handling. The composition root dispatches
  `REPORTER_ARGUMENT` before ordinary bootstrap and passes its remaining arguments to the terminal
  `run_reporter` entry with `crash_reporting::present` as the report callback. `present` accepts
  only the system's bounded report extent, owns the independent GPUI lifetime and returns after
  that surface exits. No ordinary application service is an input to these entries.
- Public commands and completions carry every applicable home, service, window, thread, draft,
  session, operation, runtime, root, connection, CAS, turn, item, revision, generation, and request
  identity. A stale or ambiguous result is rejected or reconciled by its typed owner and is never
  rebound by coincidence.
- Renderer-facing code consumes only prepared bounded app projections. Storage, backend, and
  provider workers return typed bounded values containing no GPUI state.
- `TranscriptProviderReader` prepares indexed resource ranges, display fragments and source
  anchors through its existing selected-history resource reservation and mutation observation.
  Requests qualify the resource revision, optional exact root and complete inner query, including
  continuation and smaller byte/descriptor allowances. The worker authenticates the published
  transcript backing, constructs the exact Syndic binding and borrows a frozen Home read for the
  bounded query. It releases that custody before returning on success or failure. Prepared results
  retain the original publication observation; publication rechecks the exact request, query,
  owner and cancellation through the existing resource election boundary. Ready results transport
  Syndic root/source facts and canonical payload charges without duplicating their binding.
  Missing companions return an explicit Pending tag (one encoded byte); failures never become
  empty Ready results.
  Exact Source reads admit Code and Table backing only, authenticate their respective structural
  companion through an empty typed query, and require the captured root. Each page contains at
  most 8,192 exact source bytes within the backing range; arbitrary byte boundaries may split a
  UTF-8 scalar. Consumers retain bounded carry when decoding contiguous pages. Source responses
  keep the same owner, revision, cancellation and publication election as structural queries.
  The host's source-action worker may apply the shared bounded stripped-Copy converter to admitted
  exact source after assembly. It retains the original selection/source identity and capacity
  custody until currentness elects publication or rejection. Conversion never runs in the widget
  renderer, discovers block structure or creates another storage reader.
  `prepare_resource_delivery` and `publish_resource_delivery_if_current` dispatch raw and indexed
  requests within the same tracked selected-history worker and publication election. The native
  resource completion stages either kind through the existing measured resource candidate.
  Residency holds at most one admitted inner query per resource, bounded by the existing request
  count policy, and checks the full inner query and current outer owner on admission. Its shared
  typed payload preserves Syndic facts without reparsing; canonical encoded bytes join raw-slice
  charges under the resource and per-response budgets. Replacement, disposal, obsolete outer
  records and activation reset release this state. An admitted Pending response remains explicit
  and suppresses repeated reads until its exact demand is refreshed. The host/panel
  `refresh_pending_indexed_resource` operation releases only an admitted Pending result for the
  exact current demand; Ready results, replaced demands and obsolete owners reject refresh. It
  restores read demand without changing outer visibility or activation, and the panel defers it
  while a measured candidate is staged. Construction completion must refresh that demand through
  the existing projection work owner. The panel rotates eligible raw and indexed reads by stable
  record identity after measured publication or rejection of the current demand. It retains one
  queue position qualified by view and activation, without retaining a query or payload. Repeated
  inner replacement or Pending refresh cannot keep an eligible peer behind that resource;
  discarded candidates do not consume a turn, obsolete activations ignore the position and
  retirement releases it.
  Table Header, Body, Source and anchor requests share this indexed owner and serialize through
  its single inner query. Header/Body continuation retains its exact query and cursor during
  response-absent frames while the measured desired rectangle remains unchanged. Header/Body
  requests use a 16,384-byte response allowance, keeping legal cross-chunk display reads within
  the existing query ledger without increasing its limits. Source, anchor and Code allowances
  retain their respective contracts. A coherent
  window requires complete exact-root, binding and rectangle agreement; standalone bounded Body
  queries remain admissible without publishing a headerless grid. Each current or replacement
  window holds at most 128 descriptors across fifteen body rows, one header row and eight columns,
  with at most 128 response pages per Table window and 256 descriptors across both windows.
  Code retains its separate sixteen-page continuation limit. Actual shared
  response allocations count once within the owner; copied viewport state is charged separately.
  Final replacement promotion retains the former coherent window's charge until measured
  publication and exact viewport adoption acknowledge its release. Another inner query is refused
  during that bounded overlap. Rejection, stale acknowledgment and disposal cannot bypass the
  existing budget or retain an unbounded third window.
- `ProjectionConnectionService::step_resource_construction` admits at most one companion
  command under the existing master command gate and ordinary projection worker capacity.
  It observes one exact durable build frontier and releases frozen custody before dispatch;
  absent builds start, Building frontiers advance with at most 65,536 source bytes and 64
  descriptors, and terminal frontiers return their explicit status without mutation.
  A definitive refusal may observe one fresh exact build frontier under the same permit and
  worker. Only an appeared or strictly advanced Building frontier proves peer progress;
  the result retains the original refusal. Unchanged or terminal frontiers do not retry.
  Cancellation before dispatch and exhausted capacity perform no command. The caller retains
  the original HomeStore command outcome, including reconciliation custody; a terminal build
  status does not authorize a Ready response, which still requires a fresh authenticated query.
  Its cloneable resource construction authority shares that exact service gate and worker pool,
  retains only a weak Home service reference between invocations and upgrades it under the scoped
  command permit. The published running-threads reader carries this authority for background work
  and rejects construction when its publication lifetime has retired. Retained authority cannot
  reopen a closed service or reserve a separate worker budget.
  The existing tracked selected-history resource worker first authenticates its read. For an
  indexed Pending response it releases the provider reservation, takes at most one construction
  step and performs a fresh authenticated read. All selected-history requests for the same
  resource revision share the deterministic zero-operation build key; finalized history uses
  this same demand path. Indeterminate commands install their original custody and trigger exact
  targeted reconciliation before rereading. Prepared output retains the original command outcome
  or reconciliation failure, handle and resolution until publication or disposal. No construction
  queue, additional worker, snapshot owner or payload cache is created.
  After measured publication, an exact Pending demand is refreshed when construction progressed,
  a bounded refusal check proved peer progress, reconciled to an exact old/new state, or lacked
  ordinary capacity. Refresh happens after the
  staged candidate is committed and the resource queue advances, preserving measurement custody
  and peer fairness. Ready reads perform no construction; refused terminal states do not retry.
- The package never leaks raw Fjall handles or encodings, backend JSON, CAS history collections,
  dependency custody internals, or storage proof construction across its public surface.
- Every app-owned queue, worker set, page set, cache, retained projection, in-flight operation, and
  reconciliation scope has an explicit count, byte, or concurrency bound and a terminal release
  path. Logical durable content is paged or streamed rather than truncated to meet resident bounds.

## Cross-Cutting Guarantees

- Resource settlement below governs ordinary operation and returned failures. Fatal application
  panics follow the crash-reporting process boundary and require no shared-state teardown or
  ordinary acknowledgement before direct termination.

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

This rigor declaration governs this entry point and all linked normative supplements. Supported-
envelope public contracts, exact external-effect custody, cancellation and replacement fences,
bounded retained state, and explicit failure outcomes require concrete verification and semantic
review; semantic loss is blocking.
