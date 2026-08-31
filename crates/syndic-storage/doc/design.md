# Goals

Provide the reusable production storage boundary for Syndic-owned durable threads, drafts,
conversation history, capture records, projections, and immutable resources inside the Beryl home.

Keep routine reads, recovery, and mutations bounded while preserving exact persisted compatibility,
atomic publication, and typed failure outcomes.

Keep Fjall access and physical home ownership behind `beryl-home-store` while exposing only typed
Syndic identities, revisions, values, reads, commands, proofs, receipts, and reconciliation custody.

## Non-goals

- Calling a model provider or owning authentication, approvals, sandboxing, live execution, stop,
  compaction, dispatch, or projection scheduling.
- Owning product policy, GUI presentation, renderer residency, scroll state, or widget types.
- Owning the physical home database, lock, writer, journal, or persistence barrier.
- Persisting credentials, cookies, bearer headers, listener capability tokens, or complete provider
  payloads.
- Treating routine open as a scrub, migration, repair, or application-record scan.

# Decisions

## Documentation Set

This entry point defines the package boundary and governs the following normative supplements:

- [`design-draft-storage.md`](design-draft-storage.md) owns durable draft, editor, piece-tree,
  marker, candidate, staging, history, materialization, restoration, and sealed-text contracts.
- [`design-history-storage.md`](design-history-storage.md) owns durable thread, conversation-history,
  capture, projection, resource, recovery, and privacy record semantics.
- [`design-schema-v7.md`](design-schema-v7.md) is the closed byte-compatibility authority for the V7
  domain, all 90 families, codecs, canonical encodings, bounds, and structural proofs.
- [`design-mutation-protocols.md`](design-mutation-protocols.md) owns package-generic revision,
  publication, replay, custody, cancellation, reconciliation, and acknowledgement-loss rules.

A supplement is authoritative only for its bounded role above. This entry controls package scope,
dependency ownership, the public boundary, and engineering rigor. The cross-package CAS lifecycle is
owned by [CAS live Syndic transcript](../../../doc/systems/cas-live-syndic-transcript/design.md);
conversation projection behavior is owned by
[Syndic conversation history](../../../doc/systems/syndic-conversation-history/design.md); physical
home semantics and proof composition are owned by
[Beryl home storage](../../../doc/systems/beryl-home-storage/design.md). User-visible thread-title
and composer behavior remain in their feature authorities.

## Storage Responsibility

- `syndic-storage` uses Fjall only through `beryl-home-store`. It has no direct Fjall dependency,
  database handle, keyspace exposure, or independent physical database.
- The package owns the `syndic` logical domain: persisted family declarations, canonical codecs,
  schema validation, typed queries, bounded mutation preparation, and package-private HomeStore
  contributions.
- `beryl-home-store` owns the database, home lock, registered-domain lifecycle, serialized writer,
  atomic multi-domain commit, persistence barrier, and targeted reconciliation mechanism.
- Stable cross-package identities and transportable value facts belong to `beryl-model`. Package
  values add only Syndic-specific ordering, lifecycle, lineage, storage, and recovery facts.
- Public serialization of a value is a transport shape, not persisted-schema authority. The V7
  supplement exclusively controls stored bytes.

## Public Boundary

The package exposes grouped typed operations rather than raw record mechanics:

- Domain registration, fresh typed-handle acquisition, and explicit schema validation.
- Thread, draft, turn, branch-context, usage, title-source, lineage, history-summary, and revision
  reads.
- Editor-candidate open, publish, dispose, exact-root read, edit-history append, undo/redo adoption,
  staging, build, settlement, materialization, restoration, and sealed-text range operations.
- Accepted-input, capture, item, activity, transcript, projection, resource, binding, execution, and
  recovery reads and package mutation contributions required by the owning system services.
- Opaque proof preparation, command dispatch custody, receipt consumption, and targeted
  reconciliation for package operations that participate in HomeStore composition.

Public APIs use stable Syndic identities and typed nonzero monotonic revisions. External CAS
identities are bounded source metadata and never the sole primary key. Every correctness-sensitive
mutation names exact expected revisions or immutable roots for all authority it reads or changes.

Cloneable handles are non-`Copy` views. Prepared commands, installation tokens, proof inputs,
receipts, final proofs, ambiguous-outcome handles, and reconciliation authorities are opaque,
move-only values. Their custody cannot be reconstructed from identifiers, digests, stored bytes, or
a later read. Same-home recovery invalidates every process-local handle and capability from the
prior service generation.

## Bounded Work And Stable Identity

- Public pages and range reads obey explicit item and byte ceilings and return stable continuation
  cursors. A logical draft, history, route generation, resource, or projection may exceed one page.
- Composite reads retain a fixed number of bounded constituents and never assemble an entire
  logical collection merely because each source page is bounded.
- Metadata records, content chunks, mutation windows, and write contributions are bounded.
  Large logical values remain paged, chunked, or tree-backed.
- Operation identities are caller-stable opaque values scoped by their documented owner. Occupied
  identities replay only from canonical byte equality; digest equality alone never establishes
  identity or success.
- Routine open validates domain and family declarations, reacquires a fresh handle and configured
  attachment, and performs only the bounded reads required by that attachment. It does not scan all
  application records, resume work, publish dispatch authority, or treat a physical family sweep as
  startup discovery.
- Exhaustive enumeration is restricted to explicit schema validation, scrub, background
  maintenance, or corruption investigation. Such work cannot itself dispatch or consume live work.

## Failure, Recovery, And Privacy

- Missing records, stale revisions, bounded-limit failures, cancellation, provider unavailability,
  occupied identities, ambiguous commit outcomes, and durable corruption are distinct typed
  outcomes. No error path publishes partial authority.
- Recovery and ambiguous-outcome classification begin from an exact caller-supplied natural anchor,
  follow only its bounded closure, and double-observe mutable anchors. Anchor drift is concurrent
  change; stable disagreement or missing required authority is corruption.
- Routine recovery discovers pending work only through explicit compact source families and
  revision-bound pages. It never sweeps the broad input-gate or history families.
- A failed service generation contributes no surviving execution authority. Fresh authority comes
  only from current durable records validated by a fresh package handle and by the owning system's
  lifecycle rules.
- Stored provider observations contain normalized metadata and references to bounded sealed ranges,
  not credentials or complete provider payloads. Sensitive text is returned only through explicit
  content or resource reads; diagnostics and errors use identities, revisions, kinds, counts, and
  redacted summaries.

# Engineering Rigor

Profile: production-application/v1

Modifiers:

- persistent-state-integrity/v1
- shared-resource-protection/v1

This rigor contract governs this entry point and all four normative supplements in the
documentation set.

The package is a persistent-state boundary shared by UI, background projection, capture, and
recovery services. Every persisted key, value, version, digest preimage, ordering rule, revision
transition, and replay classification is compatibility-sensitive. Arithmetic is checked; decoding
is canonical and fail-closed; writes are atomic; work and retained memory are explicitly bounded;
and ambiguous outcomes preserve single-owner custody until exact durable reconciliation.

Verification must cover canonical codec round trips and rejection, exact family inventory,
key/value agreement, byte-order and digest-domain vectors, revision conflicts, replay collisions,
acknowledgement loss, cancellation at each admission boundary, bounded traversal, corruption
injection, routine-open no-scan behavior, and multi-domain atomicity. Review must include an
adversarial pass for split publication, stale or substituted authority, digest-for-byte-equality
confusion, unbounded traversal or allocation, secret leakage, and capability duplication.
