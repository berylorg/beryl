# Live Capture

This supplement is normative only for its bounded beryl-app live-capture role and is governed by
[design.md](design.md). It does not independently declare engineering rigor.

Normal capture, outage, terminal, repair, and recovery policy belongs to the
[CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md). Canonical records and
fixed points belong to the
[Syndic conversation-history system](../../../doc/systems/syndic-conversation-history/design.md).
This supplement owns app buffering, ordered ingestion, custody transfer, capture adaptation, audit,
and bounded repair coordination.

## Outage Buffer

- The ordered ingester owns a separate unpublished-observation assembler for CAS's late route.
  It retains typed fields and controls, never raw operations, response capabilities or generic
  JSON. One observation per connection has explicit total-byte, per-field-byte and entry limits;
  admitted connections bound aggregate usage. Healthy capture continues using Syndic staging.
- Begin fixes the observation schema. Fragments must agree on item, kind, field, ordinal,
  protocol indices and contiguous offset; a field is eligible only after its explicit closure.
  Seal validates the trailing route against the connection's frozen outage targets before
  borrowing complete facts into the retention component. Assembler storage is released after
  handoff, rejection, abandonment, cancellation or retirement.
- Overflow and a mid-observation store failure retain only bounded loss state until route or
  connection termination. They never submit a retained suffix as a complete field. Valid route
  marks its exact target gapped; unavailable or unqualified route marks the frozen active targets
  of that connection gapped. A malformed observation terminates capture rather than guessing.
- Outage-mode acknowledgements mean transient consumption only. Indeterminate home custody must
  reach reconciliation before entering this mode; the durable admission fence remains closed.
  Retirement cancels ingress and joins the consumer without waiting for provider completion,
  then disposes both scratch and qualified retention before constructing replacement services.
- One coordinator owns the fixed-capacity process-local outage buffer for normalized observations
  from already active exact targets during temporary durable unavailability.
- Configuration fixes item, encoded-byte, per-field-byte, and target-count limits and prioritizes
  identity, lifecycle, and terminal evidence. Backpressure and typed overflow apply; buffered data
  is never durable or canonical.
- Dropped, evicted, partial, unrepresentable, or rejected canonical facts mark the exact turn
  capture-gapped for system convergence. A retained suffix or terminal never proves completeness.
- Store failure fences new durable admission and retires the service generation. Buffered facts,
  connections, and process-local authority do not transfer to replacement.

The private retention component freezes the complete admitted target set before accepting facts.
Each target includes Syndic and CAS turn identities together with runtime, home, process,
connection, loaded-session and registration identity. Target metadata and sticky gap flags are
non-evictable and count toward the byte limit. Construction rejects an oversized or duplicate
target set rather than silently omitting targets. The caller establishes active-target custody;
these descriptive values do not grant it.

The component borrows closed normalized lifecycle, control, terminal, identity, user-correlation,
saved-path and complete-field values. It checks a compact encoding with fixed tags, exact indices,
length-prefixed UTF-8 and all target metadata before copying any retained payload. Controls carry
the retention priority of their normalized semantic field. It accepts only complete bounded
fields; partial fields, unrepresentable observations and dropped facts report explicit loss for
their qualified target. Fragment assembly and pre-route attribution are outside this component;
the ingester must not submit fragments as complete fields or guess a route.

Admission first proves that enough strictly lower-priority facts can be evicted. An impossible
admission marks the incoming target gapped without evicting retained facts. Eviction chooses the
lowest priority first, preserving arrival order among survivors and marking every victim's target
gapped. Equal-priority saturation rejects the incoming fact. A gap never clears during this
buffer's lifetime. Retirement consumes the sole buffer and releases its transient contents;
neither its bytes nor its target descriptors constitute durable evidence or a repair capability.

## Ordered Ingester And Custody

- Each connection's sole ordered ingester consumes the closed compact-control, approval,
  dynamic-tool, and provider-operation union directly into typed owners. Source publication and
  acknowledgement remain ordered through the capacity-one broker.
- The exact process execution session owns that consumer for both viewed and unviewed threads.
  Window detachment releases presentation subscriptions only. No background thread requires a
  transcript/editor mount or additional consumer; capture and terminal-history work retain their
  ordinary bounded publication and custody until exact execution retirement.
- Provider begin allocates one observation identity as a 128-bit value from the OS cryptographic
  random source. Every fragment, control, seal, reconciliation, and publication is fenced by it and
  exact route, generation, registration, item kind, field, ordinal, and protocol index.
- A route-bound seal acquires one non-cloneable publication permit, releases the router gate, and
  reaches durable or terminal failure before acknowledgement. Target close abandons unpublished
  work or waits for the winning permit; it never redirects observations.
- Provider-staging `Indeterminate` is the ingester's immediate custody. Before reply,
  acknowledgement closure, release, cancellation observation, or retirement, it moves the sole
  opaque descriptor and reserved capacity into the current home's reconciliation registry.
- Consuming-seal custody uses one move-only guard that installs home custody before releasing its
  inert stager. It exposes no receipt, successor, retry, or publication authority, and fail-closed
  destruction cannot discard custody.
- Registry handoff publishes no acknowledgement, source fact, retry, reread, rollback, or
  reconciliation execution. Later authority comes only from typed natural records.

## Ordered Bounded Publication

- Capture maps the complete closed normalized item union to borrowed typed views and streams one
  bounded provider-field fragment at a time into Syndic content. It never reduces items to generic
  text, fieldless activity, raw JSON, or a whole app-owned value.
- Raw reasoning and standalone image-generation base64 are excluded at ingress. Supported
  `savedPath` and nonbinary lifecycle metadata remain typed; discarded base64 is never retained,
  decoded, or reconstructed.
- Each fragment validates exact CAS thread, turn, item, kind, field, ordinal, protocol index, and
  logical frontier. Replay requires stable identity and byte agreement; conflict yields the typed
  system issue or failure instead of overwrite.
- The healthy live path retains at most one bounded pending field fragment and uses bounded stabilized
  reads. It owns no active/completed item maps, whole-response clone, generic event queue, or
  approximate parsed-byte authority.
- Terminal capture flushes pending work and audits admitted items through fixed-size cursor pages
  before `TurnEnded`. Exact provider outcome and any repair reason publish together before
  acknowledgement; the app invents no backfill and does not change provider completion.
- Loss preserves every published prefix and enters source-loss convergence only after permits settle.

## Audit And Repair Adapter

- Audit compares terminal controls with exact item and lifecycle frontiers through bounded pages.
  Duplicate, reversed, kind-conflicting, incomplete, or mismatched evidence is routed to the typed
  system-owned issue, repair-required, or incomplete outcome.
- One repair coordinator owns the bounded single-turn response, item/content pages, generated-media
  preparation, and final typed cross-domain publication for an already authorized exact target. It
  cannot authorize historical access or broaden scope.
- Repair accepts only the exact typed adapter result and stages no partial canonical publication.
  Generated-media work is keyed by durable resource identity, has at most one flight per identity,
  and uses configured finite queue, worker, page, and byte capacities.
- Failure, cancellation, loss, incomplete response, media failure, ambiguity, retirement, and
  replacement release all app pages, handles, permits, and response custody. Durable inert staging
  remains under system authority.
