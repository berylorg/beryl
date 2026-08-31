# Live Projection And Scheduling

This supplement is normative only for its bounded beryl-app live projection and scheduling role
and is governed by [design.md](design.md). It does not independently declare engineering rigor.

The [CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md) owns projection,
lineage, dispatch, steering, scheduling, and recovery policy. This supplement defines app-owned
topology and typed execution surfaces.

## Runtime Interest And Connections

- Runtime readiness is process-wide and coalesced by exact runtime/root demand while projected per
  window. Releasing one window preserves all other interest and required work; final release drains
  required work before orderly retirement.
- Every validated connection has one non-GPUI driver, one ordered ingester, one non-cloneable sink,
  one bounded router, and one capacity-one broker with one acknowledgement slot and at most one
  current bounded operation. Only the driver polls and sends serialized provider requests.
- The service acquires the configured driver-and-ingester permit pair atomically before either
  starts. Construction failure, retirement, and shutdown release the pair.
- Workers receive only typed admitted capabilities. They cannot poll the stream, inspect backend
  storage, parse raw JSON, or retain GPUI, home-store, repository, or window handles.
- A connection retains at most 256 retired remote-thread lane fences. Overflow retires the
  connection rather than forgetting exclusion. Other queues, prefixes, registrations, and worker
  sets use configured finite capacities, not tuning values as semantic authority.

## Projection Authority And Leases

- A projection is usable only with exact home, service, runtime, managed-process, connection,
  loaded-session, Syndic thread, selected prefix, binding, and tool-profile authority. Equal ids or
  another connection's observation never substitute.
- Per-thread subscription leases come from one process-wide generation allocator. Each binding use
  requires one current admitted lease; release revokes locally before bounded unsubscribe. Drop
  performs no backend I/O and cannot leave reusable authority.
- Retirement linearizes with registry acquisition through one bounded in-memory gate containing no
  backend or storage work. Connection or process loss revokes matching leases and registrations.
- Native continuation, resume, inclusive fork, fresh lineage, or one-time recovery injection is
  selected from bounded typed proofs. The app never dispatches rollback, summarizes a prefix,
  assembles recovery history, or silently selects injection after unclassified native failure.
- Recovery creates one exact 65,536-byte page and both capacity-one broker directions before its
  fresh target. The coordinator derives the system-defined domain-separated source identity from
  the exact home, thread, selected path, represented prefix, source revision, totals, and sequence
  digest, then transfers the sole page between typed storage and connection workers without
  assembling a recovery sequence or backend batch.
- Cancellation, broker unavailability, revision drift, dependency-read failure, and invalid
  durable source or proof remain distinct typed adapter failures. Both broker sides receive one
  terminal result, and every terminal cut releases the page, rings, target, and lease.
- Immediately after exact recovery-injection success, the app coordinator obtains the typed Unix
  completion timestamp used by durable publication. A clock before the Unix epoch, a value outside
  the durable range, or another typed clock observation or conversion failure abandons the fresh
  unpublished target and returns no capability or publication. The coordinator never substitutes
  the earlier request timestamp and does not retry injection merely to obtain another time.
- Failed or ambiguous fresh-target publication submits the exact opaque publication scope for
  targeted natural-record reconciliation and returns no projection capability. Only the system's
  `ExactNew` classification and reread of the named eligible binding may establish another fresh
  projection; uncommitted or stale target provenance remains non-authorizing and is never promoted
  by resume or reinjection.
- Caller usability begins only after exact durable binding publication.

## Outbound Preparation

- Pending-turn and accepted-input execution share one bounded range-backed engine retaining compact
  source identity, immutable proof, count/digest header, and cursor/pass state. It never assembles
  composer content or an image list in app memory.
- The text broker has capacity one and answers one bounded absolute-page request at a time only
  through retained source authority. Marker/asset preparation retains one current marker, sealed
  reference, verified sidecar, and runtime path at a time.
- The accepted-input correlation codec accepts exactly `beryl.accepted-input.v1:` followed by the
  identity's 32 lowercase hexadecimal digits. No other prefix, case, width, or encoding is valid.
- Pre-activation preparation failure returns the same exact still-live projection. Consumption
  occurs only after preparation and execution readiness; retry remains valid only in the unchanged
  healthy generation.
- The connection ingester is the sole live publisher of source-producing compact controls. Results
  become visible only with the typed broker proof required by activation or lifecycle.

## Scheduling And Steering

- One process-owned level-triggered scheduler reads revision-bound durable pages for steering,
  accepted-next, and recovered-pending work. Durable routes own backlog; resident state is bounded
  to compact cursors, lane wake facts, one candidate per permit, and join/disposition facts.
- Steering and ordinary lanes share a coalesced wake service and bounded worker pool while retaining
  distinct eligibility and permit authority. A wake or release cannot be retyped across lanes.
- Each ordinary candidate acquires its worker permit before claim and one exact same-thread flight
  through validation, promotion reconciliation, projection establishment, dispatch, and terminal
  disposition. Saturation mutates no route and creates no backlog.
- Promotion uses one exact service lease and one atomic typed home command for Syndic promotion and
  Asset-owner transition. `Prior`, `Exact`, collision, and unresolved outcomes settle before any
  provider work; only exact promotion proceeds.
- A connection-wide non-cloneable steering attempt is retained from Ready-to-Delivering through
  exact success, retry, structured rejection, delivery-unknown, or target loss. Possible dispatch
  is never automatically replayed.
- Completion opens another pass only through typed continuation or a fresh durable, execution,
  recovery, flight, cancellation, or relevant external-capacity wake. No timer retry, per-input
  retry set, payload waiter, or self-wake loop exists.

## Fresh-Service Establishment

- The unpublished fresh stack performs bounded durable pending, stop, compaction, terminal-history,
  and repair convergence off the GPUI thread before scheduler, projection acquisition, admission,
  or execution wakes open.
- A sealed recovery-ready handoff is consumed only after supervisor attachment and atomic whole-
  stack publication, opening fresh lanes and projection establishment from durable authority.
- Old-generation schedulers, connections, flights, leases, workers, projections, and route
  authority are joined and released. Durable work remains for the new service; readiness never
  turns the old service into replacement authority.
