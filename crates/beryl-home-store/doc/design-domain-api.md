# Domain API

This supplement is normative only for domain registration, live type and attachment identity,
bounded typed reads and results, and read-only proof composition. It is governed by
[the package design](design.md), including that design's engineering-rigor contract.

## Registration And Type Ownership

- A domain registers private record families, exact codecs, exhaustive validation and
  operation-scoped reconciliation hooks, typed reads and bounded mutation preparation, optional
  validation-only participants, and exactly one process-local runtime-attachment type and factory.
  Registration never supplies a raw database or keyspace handle.
- Every live blueprint, handle, command contribution, proof role, runtime attachment, and recovered
  registration carries the exact Rust domain-owner type. Every family carries the exact codec type.
  Stable names and schemas are durable declaration facts but cannot impersonate live Rust owner,
  codec, attachment, store-instance, or generation identity; no Rust `TypeId` is persisted.
- Stable domain and family identifiers are bounded nonempty lowercase-ASCII components. The
  persistent registry records the exact nonzero domain schema, complete sorted family declaration,
  exact nonzero family schemas, exact derived physical names, and current domain revision. Physical
  names are exactly `d.{domain}.{family}` and are not themselves logical identifier components.
- Persistent domain metadata has an 8-KiB stored-byte ceiling. Encoding V1 stores the family count
  as `u16` in a fixed 26-byte envelope. Each family entry uses one-byte lengths for strings bounded
  to 1 through 255 bytes; its minimum valid 12-byte form contains a one-byte logical identifier,
  the five-byte minimum derived physical name `d.{domain}.{family}`, both length bytes, and a
  four-byte nonzero schema. The envelope therefore admits at most 680 families and has no separate
  fixed family-count policy. Encoding and decoding reject a count above 680 before family traversal
  or allocation, bound allocation by the validated count, and then enforce the exact final metadata
  ceiling and declaration match.
- Routine open, recovery, and handle reacquisition validate exact live owner and codec types,
  generation, durable declarations, and required physical families without exhaustively scanning
  application records. Registering a persisted domain at a schema-validation boundary validates
  every snapshot-current record through its exact codec and then runs its bounded sidecar-aware
  domain validator before publication. Fresh registration persists an empty declared domain.
- Because Fjall has no keyspace deletion or multi-keyspace creation transaction, interrupted fresh
  registration may leave empty physical families before registry publication. A later exact fresh
  registration may adopt only an empty same-name family. A nonempty unregistered family or an
  nonmatching declaration is structural failure, with no cleanup, rename, or adapter path.
- Stored values carry the package-owned exact record-version prefix. Domain codecs remain private
  to their owning packages.

## Runtime Attachments

- Registration or recovery supplies `DomainRegistrationReader<'a, D>` only for the provisional
  exact domain `D`, after its durable declaration and every physical family are open and validated
  but before its live slot or handle is published. The reader provides bounded domain-local reads
  on one coherent snapshot and no other-domain, write, registry, or raw-storage authority.
- The `StorageDomain` attachment factory borrows that reader and must return a fully initialized
  attachment before publication. Failure publishes no slot, live registration, attachment, or
  handle and drops the provisional state.
- Each published slot is the sole strong owner of exactly one attachment for its home generation.
  Typed handles are cloneable, non-`Copy`, non-owning views that reacquire that same attachment only
  after store, owner, registration, and generation fences. Cloning or routine reacquisition cannot
  construct another attachment or extend its lifetime.
- Generation retirement closes new attachment access, drains admitted users, and synchronously
  retires and drops every attachment exactly once. Surviving handles and attachment-derived
  capabilities become stale non-owning views and cannot authorize access or delay retirement. A
  recovery candidate constructs fresh attachments and cannot transplant prior-generation identity
  or custody.

## Bounded Reads And Results

- Point, cursor, registration, validation, preparation, reconciliation, and proof reads use one
  coherent Fjall snapshot. Public reads require explicit key, item, stored-byte, decoded-byte, or
  finite range bounds unless their result is a documented fixed-size record set.
- Point and cursor execution validates stored key and versioned value envelopes and uses checked
  arithmetic for configured per-record, item, cumulative stored-byte, and decoded-byte limits.
  Stored-value limits are checked from metadata before separated-value acquisition; decoded limits
  pass before a result or page is published.
- A point read returns one ordinary typed value or absence. A cursor takes two finite typed
  endpoints and returns at most one caller-bounded typed page, its checked stored and decoded totals,
  continuation state, and whether more matching records exist. It never exposes a Fjall iterator or
  guard. Retaining or caching pages remains the caller's bounded responsibility under the
  [bounded-resource system](../../../doc/systems/bounded-resource-dataflow/design.md).
- Caller-supplied bounds and malformed physical envelopes remain distinct errors. A caller limit
  leaves health unchanged. A stored-envelope violation observed by an admitted ordinary read fails
  the affected generation structurally before another dependent result may publish.
- Exhaustive validation alone may use the package-owned unbounded-endpoint physical cursor. It
  streams every snapshot-current application envelope through bounded pages and compact invariant
  state, including malformed or sentinel keys outside ordinary typed ranges, without exposing
  retired versions or tombstones or retaining a whole-domain collection.

## Read-Only Proof Composition

- `HomeProofCommand<P>` and `HomeStore::compose_proof` form the generic process-local boundary for
  composing private facts from distinct live domains without mutation. A command has exactly one
  statically typed source role and zero or more statically typed witness roles, at most one role per
  exact domain owner. Each role fixes its protocol and fixed-size inline `Copy + Eq` correlation,
  reads only its own registered domain, and owns its private contribution.
- The package supplies a zero-state generic fixed-digest protocol marker parameterized by exact
  `u64` protocol and operation identifiers. It implements `HomeProofProtocol` with an exact 32-byte
  correlation, allowing isolated domain packages to name one Rust protocol type without depending
  on each other. It carries no domain value, callback, registry, or durable identity.
- Sealing fixes one unforgeable process-local plan identity, home generation, protocol, operation,
  role and owner set, revision fences, callbacks, and expected correlation. It returns exactly one
  opaque one-shot executable command and its paired move-only `ProofReceiptConsumer<P>`. The source
  requester retains the consumer; neither half can mint, recover, or substitute for the other.
- Composition is independent of the serialized writer. It fixes one coherent snapshot, revalidates
  the current generation, caller-fenced domain revisions, every live owner, and complete persistent
  registrations, then accepts only equal source and witness correlations; the source-only shape is
  valid. Duplicate domains, stale fences, missing registrations, access or callback failure, and
  disagreement reject determinately. A concurrent atomic write is wholly before or after the
  snapshot and may cause later staleness or conflict, never false proof authority.
- Composition consumes only the executable and returns only an opaque fixed-size generation-bound
  receipt. Exact consumption moves the retained consumer and receipt, rechecks their sealed pairing
  and current generation, and returns only match or rejection. Neither result returns expected
  facts or a consumer, and the receipt grants no publication authority.
- Proof composition performs no registration or schema write, mutation, batch, `SyncAll`, domain or
  home revision advance, sidecar action, commit receipt, reconciliation reservation, or
  `Indeterminate` classification. Cancellation may win only before admission; admitted bounded
  callbacks finish with one determinate result on their snapshot.
