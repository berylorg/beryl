# Goals

Provide the reusable production storage boundary for Syndic-owned durable threads, drafts,
conversation history, capture records, projections, and immutable resources inside the Beryl home.

Keep routine reads, recovery, and mutations bounded while preserving the exact persisted V7 format,
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
- [`design-schema-v7.md`](design-schema-v7.md) is the sole persisted byte-format authority for the V7
  domain, all 103 target families, codecs, canonical encodings, bounds, and structural proofs.
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

- `RepairRequestDisposition` describes an available request or a `ConsumedRepairRequest`.
  `ConsumedRepairRequest::new` retains the exact `RepairRequestAttemptNonce` (16 bytes) and accepts only
  nonzero `InputGateRevision` values whose successor is the checked next source revision.
  These copyable facts carry no dispatch authority; only the owning atomic claim may grant the
  non-cloneable capability specified by the CAS-live system. Construction neither generates a
  nonce nor establishes its freshness, and does not persist or reset a request disposition.
- `RepairRequiredTarget` combines an exact typed Syndic turn and bounded `CasTurnSource` with
  `RepairCaptureGap` and the request disposition. A gap retains the exact terminal status and
  sequence/digest witness, a closed `RepairCaptureGapReason`, and an optional preceding issue
  witness. Construction rejects unknown terminal status, missing required gap evidence and
  invalid issue ordering. `RepairSourceEventWitness` and `RepairSourceEventDigest` are descriptive
  values, not authenticated reads or capabilities. Storage admission authenticates their retained
  source records under the schema contract; constructors perform no I/O or digest computation.
  `SourceEventRecord::repair_witness` computes a descriptive sequence/digest witness over the
  schema-defined domain and canonical source-event payload. It neither reads storage nor proves
  that the record is retained, eligible repair evidence or current gate authority.
- `RequireTerminalRepair` admits an available target only from its exact `FinalizingHistory`
  gate revision. `require_terminal_repair` and `current_require_terminal_repair` authenticate
  retained target evidence and absence of resolution in the admitted read, preserve queued input
  accounting, and atomically advance the gate and non-idle source. Admission grants no dispatch
  capability. Repair gates exclude successor and lineage publication across the blocked target.
  Deferred repair startup classification is descriptive; convergence must authenticate retained
  evidence again in its mutation before permitting publication.
- `ConvergeRepairIncomplete` names the exact repair target, gate and turn-state revisions, closed
  incomplete reason and monotonic timestamp. `converge_repair_incomplete` and
  `current_converge_repair_incomplete` authenticate the retained gate target in the admitted read,
  then atomically retain its unchanged request disposition in the resolved turn state, advance
  the gate to `FinalizingHistory`, preserve queue accounting and invalidate transcript publication.
  They preserve original terminal and issue records and select no repair snapshot or asset.
  Stale replay cannot rewrite the resolution; ambiguous publication uses the home-store exact
  reconciliation reservation for the state, gate/source, summary and affected transcript records.
  Resolved provenance validation authenticates the original evidence and origin thread without
  requiring that turn to remain its thread's current tail.
- `ResolvedRepair` pairs the original target with `RepairResolution::Incomplete`, carrying a
  closed `TurnIncompleteReason`. `TurnStateRecord::resolved_repair` exposes the optional retained
  value; `with_resolved_repair` checks local identity, frontier and status agreement and refuses
  replacement of an existing different resolution. These are descriptive record operations;
  neither construction nor attachment publishes a mutation or authenticates retained evidence.
  Resolved repair belongs to the existing turn-state authority. Its bounded immutable extension
  preserves the original `RepairRequiredTarget`, request disposition and closed incomplete
  resolution after gate exit. The package atomically installs it with incomplete convergence and
  preserves it through later state updates. Its presence excludes repair re-entry for that turn;
  descriptive construction or decoding grants no mutation or request authority.
- Domain registration, fresh typed-handle acquisition, and explicit schema validation.
- The complete typed declaration of the Syndic domain, candidate-only initial registration and
  fresh candidate handle acquisition. Candidate startup recovery consumes explicit home-store
  candidate access while preserving the same compact sources, typed mutations and bounded outcomes.
- `SyndicStorage::required_domains` returns the package declaration. `register` and
  `register_with_schema_validation` take a mutable `HomeOpenCandidate` reference and reconstruct
  cleanup attachment custody within that same candidate generation.
- Thread, draft, turn, branch-context, usage, title-source, lineage, history-summary, and revision
  reads.
- Editor-candidate open, publish, dispose, exact-root read, edit-history append, undo/redo adoption,
  staging, build, settlement, materialization, restoration, and sealed-text range operations.
- Accepted-input, capture, item, activity, transcript, projection, resource, binding, execution, and
  recovery reads and package mutation contributions required by the owning system services.
- `latest_item_projection_build` and its candidate counterpart return at most one item-owned
  build using a reverse indexed seek under the caller's byte bound. A published head does not name
  later abandoned construction generations. Callers distinguish active parsing from superseded
  progress and stabilize this observation with their exact source/head reads before mutation.
- `pending_dispatch_evidence` supplies home/generation/revision-bound pending identity, canonical
  input references and authenticated dispatch provenance to ordinary execution and recovery. Its
  bounded read snapshot does not settle live execution or cleanup custody; those obligations remain
  with the [CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md).
- `pending_dispatch_evidence_candidate` supplies the same stabilized bounded facts through borrowed
  home-store candidate recovery access. It preserves exact gate/source, binding, canonical-input
  and cancellation-provenance checks and grants no ordinary execution or cleanup authority before
  complete service publication.
- `terminal_history_evidence` binds the exact selected terminal turn to its home, generation and
  revision after the idle gate and the same bounded fixed point used by terminal-history release
  are established. Complete and authority-loss incomplete history remain distinct lifecycle
  outcomes. This read likewise grants no live cleanup or process-shutdown authority.
- `terminal_history_evidence_candidate` shares those exact fixed-point, gate/source and revision
  checks through borrowed candidate recovery access. Initial and reopened candidates retain the
  same bounded evidence and lifecycle distinction without releasing ordinary execution or custody.
- Opaque proof preparation, command dispatch custody, receipt consumption, and targeted
  reconciliation for package operations that participate in HomeStore composition.
- Activation-cancellation outcome reads support ordinary and explicit candidate access through
  the same bounded classifier. They authenticate the activation snapshot, unchanged-authority
  successor, CAS reservation and membership, turn provenance and gate/source under a stable domain
  revision. Candidate inspection uses fresh handles and grants no dispatch or retry permission.

Public APIs use stable Syndic identities and typed nonzero monotonic revisions. External CAS
identities are bounded source metadata and never the sole primary key. Every correctness-sensitive
mutation names exact expected revisions or immutable roots for all authority it reads or changes.

The `indexed_resource` namespace exposes the Code/Table binding, root facts, coordinate demand,
continuation and source-mapped presentation values. `SyndicStorage` provides
`read_frozen_indexed_resource_range`, `read_frozen_indexed_resource_display` and
`read_frozen_indexed_resource_anchor` under a caller-owned
`FrozenHomeRead`. These reads borrow snapshot custody; they do not capture, retain or release an
observation. Results distinguish a pending companion from a ready authenticated root and bounded
response. Continuations require their exact root even when the caller omits a separate root argument.
The total encoded response allowance includes the canonical root and result framing as well as
binding, descriptors, mappings, display bytes and continuation. Released or foreign observations
remain typed read failures. These storage reads do not establish provider or residency admission.

Anchor responses repeat the exact binding and selected root, charge the descriptor, absolute source
mapping and resolved source/visual position, and distinguish an invalidated anchor from a pending
companion. Resolution uses bounded neighboring index seeks under the same frozen observation.

`read_frozen_indexed_resource_build` reads one bounded persisted frontier into opaque `BuildState`.
The state exposes its build key, revision and lifecycle; it carries no snapshot custody. The
`current_start_indexed_resource_build`, `current_advance_indexed_resource_build` and
`current_stop_indexed_resource_build` methods create ordinary HomeStore commands. Advance consumes
the observed frontier and caller quantum, selecting the next source/output step, pass transition or
atomic completion from that frontier. Stop consumes it with typed cancel, supersede or refuse.
Mutation callbacks authenticate the exact stored frontier again before contributing; stale state
cannot write. Existing bounded mutation reservations and acknowledgement-loss reconciliation apply
to these commands. Public callers do not supply parser checkpoints, index pages or publication roots.

The normal provider-frame API performs streaming structural validation. The optional
`provider-frame-test-support` feature exposes the bounded complete materializer only through a
test-support namespace for independent canonical and malformed-frame parity checks. Default builds
neither compile nor export that decoder. Staging, publication, recovery, and projection use the
streaming path; test-support materialization cannot become a second production validation path.
The oracle preserves exact V7 bytes, history-support outcomes, and content-reference evidence.

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

Profile: production-application/v2

Modifiers:

- persistent-state-integrity/v2
- shared-resource-protection/v2

This rigor contract governs this entry point and all four normative supplements in the
documentation set.

The package is a persistent-state boundary shared by UI, background projection, capture, and
recovery services. Every persisted key, value, version, digest preimage, ordering rule, revision
transition, and replay classification is part of the exact V7 format. Arithmetic is checked; decoding
is canonical and fail-closed; writes are atomic; work and retained memory are explicitly bounded;
and ambiguous outcomes preserve single-owner custody until exact durable reconciliation.

Verification must cover canonical codec round trips and rejection, exact family inventory,
key/value agreement, byte-order and digest-domain vectors, revision conflicts, replay collisions,
acknowledgement loss, cancellation at each admission boundary, bounded traversal, corruption
injection, routine-open no-scan behavior, and multi-domain atomicity. Review must include an
adversarial pass for split publication, stale or substituted authority, digest-for-byte-equality
confusion, unbounded traversal or allocation, secret leakage, and capability duplication.
