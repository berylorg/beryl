# Goals

Own the reusable physical storage and process-ownership boundary for one Beryl home.

Provide typed, revision-checked, crash-durable coordination across registered Syndic and Beryl metadata domains without exposing Fjall internals to application or backend packages.

## Non-goals

- Owning conversation, draft, window, runtime, root, settings, catalog, job, or asset product semantics.
- Owning GPUI state, CAS launch, backend protocol, transcript rendering, or feature behavior.
- Importing workspace-era state or exposing compatibility reads and dual writes.
- Allowing callers to retain raw Fjall handles, keyspaces, batches, encodings, or writer guards.
- Detecting or surviving external replacement, rollback, or tampering inside an Operator-selected
  trusted home. Surfaced or validation-visible disagreement still fails closed.

# Decisions

## Public Boundary

- The package opens and locks one Beryl home according to `doc/systems/beryl-home-storage/design.md`.
- It owns the single Fjall `Database`, logical keyspace registration, serialized writer, persistence
  barriers, store-health state, and bounded typed read execution. Read, staging, writer, scan, and
  result paths enforce the practical limits defined by
  `doc/systems/bounded-resource-dataflow/design.md`; the package owns no universal process
  governor.
- The sole home opener requires the configured Fjall block, value, topology, cache, memtable, read,
  batch, and reconciliation-descriptor byte limits needed by this boundary. It does not require
  typed process resource capabilities, universal structural-slot currency, or an exact allocation
  baseline.
- V1 uses one package-owned practical production profile for those dependency and
  storage-concurrency limits, including exactly 1,024 reconciliation-scope slots per home, a
  64-MiB encoded-byte ceiling per descriptor, a 256-MiB aggregate retained descriptor-budget
  ceiling, and at most four active reconciliation workers.
  `HomeOpenOptions` remains the package-owned path-and-schema input and never accepts or exposes a
  Fjall policy type. The opened store retains the validated profile and constructs a fresh Fjall
  configuration from the same values for every same-home recovery, so later internal tuning does
  not churn Beryl-state or Syndic caller APIs.
- Logical domains register private record families, exact codecs, exhaustive validation hooks,
  operation-scoped natural-record reconciliation hooks, typed reads, typed mutation contributors,
  and typed validation-only command participants through package-owned traits.
- Registration never gives a domain a raw database or keyspace handle.
- Each live domain blueprint, handle, command contribution, and reacquired recovery registration carries the exact process-local Rust owner type. Each family likewise carries the exact process-local codec type. Stable names and schemas remain durable compatibility facts, but cannot impersonate either live Rust owner; neither `TypeId` is persisted.
- Stable domain and family identifiers are bounded lowercase ASCII components. The persistent registry records the exact domain schema, complete sorted family declaration, exact family schemas, physical family names, and current domain revision; reopening rejects missing families or any incompatible declaration instead of creating or guessing it.
- Persistent domain metadata has an 8-KiB stored-byte ceiling as its primary capacity bound and no
  independent fixed family-count policy. Before allocating or iterating families, encoding and
  decoding derive the conservative count ceiling as the lesser of the `u16` count maximum and the
  number of minimum valid family entries that fit after the fixed envelope. In encoding V1 the
  envelope is 26 bytes. Its minimum valid entry is 12 bytes: one byte of logical family identifier,
  the five-byte minimum exact derived physical name `d.{domain}.{family}`, two one-byte string
  lengths, and a four-byte nonzero schema. The derived ceiling is therefore 680 families. Decoding
  rejects a malformed count above 680 before allocation or entry traversal, and allocation is
  bounded by the validated count. Encoding applies the same derived ceiling before entry traversal.
  Both paths then enforce the exact final 8-KiB ceiling, the `u16` count representation, the logical
  identifiers' existing nonempty lowercase-ASCII component grammar and length bounds, every stored
  string's 1-to-255-byte bound, the physical names' exact `d.{domain}.{family}` derivation and
  declaration match, nonzero schemas, and complete exact declaration matching. Physical names
  contain separators and are not themselves lowercase-ASCII components. The format admits a valid
  73-family domain, while a 681-family count or any exact encoding above 8 KiB is rejected.
- Routine open, reopen, and typed-handle reacquisition validate the exact live owner and codec
  types, durable declarations, required physical families, and generation. They do not run the
  exhaustive record or domain validator.
- Registering an already-persisted domain at a schema-validation boundary exhaustively validates every snapshot-current
  application record in every declared physical family and then runs its sidecar-aware domain
  validator before publishing a typed handle. Fresh registration persists an empty declared
  domain. Only a schema-validation boundary, explicit whole-home scrub, background maintenance, or
  corruption-evidence investigation may rerun that exhaustive path; routine and ambiguous recovery
  do not. Retired LSM versions and tombstones are dependency internals, not application envelopes.
- An interrupted fresh registration may leave one or more empty physical families before the
  registry record becomes durable because Fjall intentionally exposes no keyspace deletion or
  multi-keyspace transaction. A later exact fresh registration may open and adopt only an empty
  same-name family before creating the remaining declarations and publishing the complete registry
  record. A nonempty unregistered family or incompatible durable declaration fails structurally;
  no cleanup, renamed schema, or compatibility route is allowed.
- Stored record values carry a store-owned exact record-version prefix. Domain codecs remain private to their package and application-facing APIs exchange only typed keys and values.
- Cross-domain commands name typed participants and expected revisions, then either commit one batch or return one typed rejection.
- The package exposes checked durable-start footprint composition that accepts only the typed idle-
  submission or accepted-input-promotion footprint from `syndic-storage` and its matching typed
  asset-owner-transfer footprint from `beryl-state`. It adds the package-owned participating-domain
  metadata, home revision mutation, and Fjall journal framing, then returns the direct or queued
  logical-and-journal append envelope. It accepts no caller-provided aggregate byte total and owns
  neither the Beryl product admission budget nor capture-reserve configuration.

## Physical Open Contract

- Open input is one absolute Host path and one exact supported home-schema version.
- The home root contains the fixed ownership file `home.lock`; the sole Fjall database lives in the
  `state` directory.
- The package canonicalizes the configured home path for process-local identity and retains the
  configured spelling only for presentation and diagnostics. It does not retain filesystem-object
  identity or deny external replacement of the home or `state` directory.
- After acquiring `home.lock` and before opening Fjall, the package rejects an existing `state`
  symlink, junction, file, or other reparse-point collision without following it, then creates or
  opens the ordinary `state` directory and validates the expected home header and schema.
- Native local NTFS is the fully supported durability tier. UNC, WSL-backed, removable,
  synchronized, and other filesystem locations may open as best-effort storage only when basic
  access and a reliable exclusive lifetime lock succeed. The package does not conditionally prove
  remote durability.
- A missing or empty `state` directory is fresh. A nonempty `state` directory must contain Fjall's version marker and is force-recovered; it is never passed through create-or-recover dispatch.
- The configured home root may not itself be a Fjall database, and `state` may not be a symlink, junction, file, or other reparse-point collision.
- The reserved home header contains one fixed-format encoding version, the exact home-schema version, and one randomly generated opaque `BerylHomeId`. Generation and first persistence occur only after the OS ownership lock succeeds.
- An opened handle exposes only the durable home id, schema, durability tier, configured and
  canonical paths, and diagnostic database path. It never exposes lock files, raw OS handles,
  Fjall database, header keyspace, or encoded header bytes.
- Explicit clean close drops Fjall ownership and then unlocks `home.lock`. Ordinary value drop does
  the same only when no reconciliation scope remains. With reserved or installed custody it drops
  disposable Fjall state while the process-local registry core retains the lifetime lock, so a
  same-process reopen remains denied until terminal classification and explicit final close or
  process termination.

## Inputs And Outputs

- Open input contains the configured home path and supported home-schema version.
- Initial open and every fresh same-home reopen first yield an unpublished private candidate, its
  full-durability or best-effort filesystem tier, and candidate-only domain registration or
  reacquisition authority. Initial bootstrap keeps that candidate behind the `beryl` composition
  root's startup fence; running same-home recovery keeps it behind the `beryl-app` home-recovery
  supervisor's publication fence. The owning boundary publishes the opaque healthy handle and
  complete typed stack only after its recovery sequence succeeds.
- The candidate remains structurally `opening` or `reopening` and exposes no ordinary command,
  read, sidecar, receipt-projection, or app-publication admission. Its one-shot publication
  capability is consumable only by the system recovery boundary with the complete candidate stack;
  bounded candidate-only reads and commits exist solely for that recovery sequence.
- Candidate authority cannot expose session, restore-set, or other application discovery through
  partial domain registration or partial typed handles. Every required Beryl and Syndic domain must
  register or reacquire successfully before the initial `beryl` composition root or running
  `beryl-app` recovery supervisor can consume the one-shot complete-stack publication capability.
- Busy, unsupported-schema, lock-unsupported, open, validation, conflict, persistence, sidecar, and health-gate failures remain distinct typed errors.
- A command returns exactly `NotCommitted { evidence }`, `Committed { receipt, later_failure }`, or
  `Indeterminate { failure, reconciliation }`. `NotCommitted` evidence proves no part of the batch
  committed and carries no receipt or descriptor. `Committed` always carries the exact receipt and
  optionally carries a typed failure observed after commit. `Indeterminate` carries the surfaced
  typed failure, no receipt, and one move-only custody value containing the sole opaque
  operation-scoped reconciliation descriptor together with its already-reserved registry slot and
  byte charge; it cannot authorize publication.
- Receipt-bound domain revision access is admitted only against the exact current healthy store generation and matching typed domain handle. It returns `None` for an unaffected domain and a typed stale-or-foreign error for an obsolete generation rather than allowing revision values alone to authorize publication.
- Read APIs require explicit item, byte, or range bounds unless the result is a documented exact fixed-size record set such as the active session header.
- Cursor reads require two finite typed endpoints, materialize at most one caller-bounded page, report cumulative stored-byte cost and whether more matching records exist, and never return a Fjall iterator or guard.
- `RecordCodec` validates stored key and value envelopes and reports a practical decoded-byte
  estimate while producing each typed result. Point and cursor execution enforce configured
  stored-key, stored-value, item, cumulative stored-byte, and decoded-byte ceilings with checked
  arithmetic. Stored limits apply before separated-value acquisition; decoded limits apply before
  a completed result is published or accumulated into a page. The package does not predict
  allocator overhead or dependency-private workspace.
- Point presence returns one ordinary typed value after its stored and decoded limits pass; it does
  not carry a resource charge or accounting wrapper. Cursor reads return one naturally bounded
  typed page with checked stored and decoded page totals plus continuation state. Empty and absent
  results remain ordinary bounded results. Callers that retain pages must apply their own
  configured page or cache limit.
- The physical dependency cursor must expose a metadata-first key or stored-length step so the
  configured value and page ceilings can reject unsafe reads before separated-value acquisition or
  caller-owned decode. Inline backing already present in a decoded block remains bounded by the
  Fjall block policy. A range API that first materializes an unbounded result or ignores configured
  LSM topology ceilings is not an implementation of this read contract.
- Read errors distinguish caller-produced key and result limits from malformed physical stored key and value envelopes. Caller limits leave health unchanged; a stored-envelope violation observed by an ordinary admitted read fails that generation structurally before another successful state-dependent result can publish.

## Atomicity And Durability

- The serialized writer validates expected revisions and each participant's exact live owner plus persistent registration immediately before commit.
- `CurrentDomainCommand` is an opaque single-domain boundary for mutations that already carry
  exact logical record fences. `execute_current` captures only that command's physical home and
  domain revisions after serialized writer admission, then uses the ordinary validation,
  contribution, batch, fault, persistence, receipt, health, cancellation, and reentry paths. It
  performs no retry and cannot combine domains or retain a sidecar token.
- `HomeCommand` remains the caller-fenced boundary for cross-domain and sidecar-retaining atomic
  work. A current-domain command is not a blind-write escape from record-level revision checks.
- A home command may include an explicitly typed validation-only domain participant alongside at
  least one mutation participant. Validation-only participants share the serialized writer
  snapshot, exact owner identity, expected domain revision, cancellation boundary, and typed
  callback-error provenance of mutations. They cannot assemble records or sidecar operations,
  advance the validated domain revision, or appear in the receipt's affected-domain revisions. A
  command containing only validation participants fails before callbacks with a typed
  `ValidationOnlyCommand` error and produces no commit receipt. A mutation participant whose
  contribution callback emits nothing still fails as an `EmptyContribution`.
- One domain may participate at most once in a home command across both roles. A domain mutation
  carries every same-domain guard in its own validation callback; combining a separate validator
  and mutation for the same domain is rejected as duplicate participation rather than assigning
  ambiguous revision or receipt effects.
- Ordinary commands run only each participant's bounded validation and mutation-contribution callbacks. They never rerun an exhaustive domain validator or scan unrelated records; one-record command work is independent of total domain size unless that participant's own documented bounded reads reject.
- A validation or mutation callback performs only operation-bounded reads. A page, item, stored-byte,
  or decoded-byte limit failure returns through typed read-error provenance without waiting while
  the serialized writer is held, mutating health, or partially assembling a contribution.
- The opener applies the validated Fjall block, value, topology, cache, memtable, and batch policy
  before opening the database. This package neither requests exact dependency residency quotes nor
  reconstructs dependency-private allocation formulas.
- Only registration at a schema-validation boundary, an explicit whole-home scrub, background
  maintenance, or corruption-evidence investigation may use the separate store-owned exhaustive
  path. It
  walks every snapshot-current application key/value envelope with bounded memory, rejects empty or
  oversized keys and values before unbounded materialization, and delegates unknown, out-of-range,
  sentinel, version, and payload validation to the family's exact registered codec before the
  domain-level invariant callback runs. The dedicated exhaustive cursor spans the complete visible
  keyspace without typed endpoints; it does not expose retired LSM versions or tombstones. Domain
  validation consumes bounded pages and compact checkpoint state; a callback may not accumulate the
  complete domain in a set or collection. Cross-record invariants use durable indexes, ordered
  traversal, staged durable proof, or another bounded validation algorithm.
- Callback errors explicitly separate typed `ReadError` or `SidecarError` access provenance from domain-owned semantic rejection. The store never guesses provenance by walking an erased error chain.
- A failure at any validation or contribution stage drops the complete uncommitted command.
- Before physical batch construction, the writer checks exact mutation count plus encoded key and
  value totals against the configured batch limits and passes that same `BatchCapacity` to Fjall.
  Oversized batches fail before commit; no exact dependency allocation quote or structural-slot
  reservation is required.
- A successful Fjall `PersistMode::SyncAll` outcome means the complete write batch committed
  atomically and is durable. The package never reports durable success before that outcome.
- Command-outcome classification uses that complete durability boundary. A Fjall `Committed` error
  from the preceding buffered journal step remains exactly retained in the typed failure but maps
  to package `Indeterminate` with a reconciliation descriptor when the later `SyncAll` barrier did
  not succeed. It carries no receipt. Only a failure after successful `SyncAll` may return
  `Committed { receipt, later_failure }`.
- Before writer admission, every mutation that could become indeterminate proves a conservative
  encoded descriptor-byte budget from command-owned identities and
  declared schema limits. After admission, validation and operation-bounded reads materialize the
  exact natural-record old state, intended exact new state, and intended receipt facts into that
  reservation before batch construction or any Fjall mutation. The package rejects a budget above
  the configured ceiling as
  `NotCommitted { evidence: ReconciliationDescriptorTooLarge }`.
- Such a mutation must then obtain one move-only reservation for one of the home registry's 1,024
  scope slots and its complete
  conservative byte charge under the 256-MiB aggregate ceiling before it may acquire writer
  admission. Slot or aggregate-byte saturation returns
  `NotCommitted { evidence: ReconciliationCapacity }`, creates no scope or descriptor, and does not
  change structural health. `NotCommitted` and `Committed` release the reservation with their
  writer-time state. `Indeterminate` first transfers the reservation and sole descriptor into its
  move-only custody value. The immediate recipient must synchronously and infallibly consume that
  value into the already-reserved exact registry gate before translating or erasing the result,
  installing an acknowledgement, releasing operation state, or honoring route cancellation. Once
  accepted, the registry is the unique owner; no result, acknowledgement, caller, or service retains
  a copy. Gate installation preserves custody and closes publication for that exact scope, but by
  itself authorizes no reread, retry, rollback, publication, or reconciliation execution. Dropping
  an unconsumed custody value performs the same infallible registry installation as a fail-closed
  fallback; it cannot release the slot, descriptor, or charge.
- Reconciliation invokes each participating domain hook through the descriptor's bounded typed
  reader and returns exactly `ExactOld`, `ExactNew` with the reconstructed exact receipt, or
  `Collision`. All participants must prove the same exact side. Any participant-level collision,
  mixed old/new classifications, or observation matching neither exact side is command-level
  `Collision` and keeps that operation scope closed.
- Reconciliation never guesses, merges, clears or crosses the old writer, scans unrelated natural
  records, or invokes whole-home validation. A `Collision` is not filesystem path collision and is
  not by itself structural store-failure evidence.
- Cooperative cancellation is accepted only before writer admission. Once admitted, a command runs to one durable success or typed failure result; same-thread writer reentry is rejected explicitly rather than deadlocking.
- Callers cannot hold transactions across await points or external work.
- Exactly one command may hold the writer-admission permit. The package owns no writer wait queue:
  callers await that permit with backpressure and may cancel while waiting. The admitted command
  releases the permit only after its classified result is constructed and its batch, callbacks,
  snapshot, and writer-time state have been dropped.

## Free-Space Reserve Check

- The package exposes one synchronous free-space reserve query that reports exactly
  `FreeSpaceOutcome::Sufficient`, `FreeSpaceOutcome::BelowReserve`,
  `FreeSpaceOutcome::Unavailable`, or `FreeSpaceOutcome::Indeterminate`. Each invocation queries the
  filesystem once and retains no polling task, timer, cache, or hysteresis state.
- The package owns the opaque validated turn-start admission requirement and immutable 256-MiB
  product policy. Its public constructor accepts separately composed direct and queued
  owner-derived `DurableStartFootprint` values plus the app-configured typed nonzero capture reserve,
  rejects drift or checked-add overflow, and accepts no arbitrary byte total. App service
  configuration obtains those owner inputs and retains the resulting requirement; the query accepts
  that type intact.
- `FreeSpaceOutcome::Sufficient` contains the observed available bytes and validated required bytes.
  `FreeSpaceOutcome::BelowReserve` contains those same exact values.
  `FreeSpaceOutcome::Unavailable` means the platform returned no availability observation;
  `FreeSpaceOutcome::Indeterminate` means an observation existed but could not be trusted for this
  home. Neither failure outcome is silently treated as sufficient.
- The query classifies one filesystem observation only. It performs no write or capacity
  reservation, retains no caller input, and does not choose call sites, operation eligibility,
  admission, retry, writer dispatch, or backend dispatch. Those cross-package decisions remain in
  the [Beryl-home storage system](../../../doc/systems/beryl-home-storage/design.md).
- The check does not reserve filesystem capacity. Later `ENOSPC` remains an ordinary storage error
  and uses the same commit-outcome classification as any other failed write. Journal rotation,
  flush, compaction, and filesystem allocation are variable dependency and platform behavior, so
  the composed append envelope is not a bound on physical consumption.

## Health And Recovery

- The package exposes the structural lifecycle states `opening`, `healthy`, `failed`, and
  `reopening`. A separate operation-scope gate has `open`, `verifying`, and `closed` states;
  `verifying` is never a structural lifecycle state.
- Every state-dependent read, write, domain registration or reacquisition, and sidecar operation is
  generation-aware. Accepting an `Indeterminate` custody value moves only its exact operation scope to `verifying`;
  `ExactOld` or `ExactNew` reopens it and `Collision` closes it. Malformed records, invalid trusted
  contracts, poisoned current authority, and other separately proven structural disagreement move
  the store to `failed`. Domain-owned semantic mutation rejection and reconciliation collision do
  not change structural health.
- Admission also observes Fjall's retained autonomous-maintenance health before a newly completed
  state-dependent result may publish. A direct pre-commit policy denial remains a typed bounded
  operation failure and leaves home health unchanged. Corruption, integrity, keyspace-identity, or
  poison disagreement fails structurally. `Committed` still returns its exact receipt when a later
  maintenance observation separately closes publication or fails the lifecycle; `Indeterminate`
  closes only its operation gate for reconciliation. The package retains the stable Fjall class
  and commit state before erasing any dependency error.
- Dependency-health observation and exact-generation confirmation are one store-owned publication
  operation. Reads, writes, registration, reacquisition, receipt revision projection, sidecar
  admission, sidecar verification, and test-only durable fixtures cannot invoke generation
  confirmation without first observing the exact admitted Fjall database.
- An unwind from an admitted writer operation moves the store directly to `failed` before writer
  admission drains. Recovery drops the poisoned writer with the failed service and creates a fresh
  writer; it does not cross or clear the old mutex poison.
- Ambiguous-outcome verification is single-flight per exact operation gate. It drains related
  admitted work, performs a physical Fjall verification or reopen when needed, and invokes only
  the descriptor's domain-owned natural-record hooks. It never requires an exhaustive whole-home
  scan merely because one commit was ambiguous.
- A verifying or collision-closed gate rejects only commands and publications dependent on that
  exact operation scope. Unrelated work remains admitted while the structural lifecycle and its
  own gates are healthy. Only separate structural evidence may move the whole store to `failed`.
- The package owns one 1,024-slot reconciliation-scope registry with one 256-MiB aggregate retained
  descriptor-budget ceiling per open home. The registry begins with home ownership, ends only after
  final home close, and outlives individual Fjall generations, failed services, unpublished recovery
  candidates, brokers, connections, and CAS-live services. Its count and byte charge include every pre-writer
  reservation, verifying gate, and collision-closed gate, not only active workers. Each
  verifying gate retains exactly one opaque descriptor; each collision-closed gate retains only its
  bounded sealed facts. Another trigger joins the retained result and adds no descriptor,
  sealed-fact copy, or queue item. Reserved custody directly retains the registry core and shared
  lifetime-lock custodian. The first installed descriptor-bearing scope self-retains that core until
  terminal classification removes the last such scope; this creates no thread, worker, or global
  registry.
- Orderly final close first rejects new reservations and drains admitted commands. It cannot dispose
  a descriptor-bearing verifying gate: the caller must join its already admitted classification or
  receive a close failure while the home remains open. Once no descriptor-bearing gate remains,
  final close may dispose collision-sealed process-local facts with the registry. Forced process
  termination publishes no in-memory command result or acknowledgement. Dropping the store or its
  pending-close error with retained scopes cannot unlock the home or permit a same-process reopen.
- One home runs at most four reconciliation workers and at most one per exact scope. When all four
  permits are held, another registered gate remains closed and awaits a worker without duplicating
  its descriptor.
- `ExactOld` and `ExactNew` remove the gate and release its registry slot, complete retained byte
  charge, descriptor, worker permit, typed reader, snapshot, pages, and hook state. `Collision`
  compacts its evidence into only its configured-byte-bounded sealed old/new identities, revisions,
  digests, and collision facts. In the same registry transition it replaces the descriptor's
  conservative retained-byte charge with the sealed facts' exact encoded-byte charge, discards the
  descriptor, retains the closed gate, slot, and replacement charge, and drops the worker permit
  plus every transient reader, snapshot, page, and hook allocation. The sealed-fact schema maximum
  is included in the pre-writer reservation, so this transition never needs new registry capacity.
  A typed worker failure also drops all transient execution state while retaining at most the one
  bounded descriptor and its original retained-byte charge in that gate.
- Registry or descriptor saturation returns the exact typed pre-writer `NotCommitted` result. It
  fails that mutation closed without changing structural health, cancelling or blocking
  already-admitted work, or blocking unrelated healthy reads and commands that require no
  unavailable registry slot.
- Recovery is single-flight and is accepted only from `failed`. It retains the lifetime lock and
  per-home reconciliation registry,
  drains and drops every Fjall and keyspace handle from the failed generation, constructs a fresh
  service, and reopens `state` without requiring filesystem-object identity continuity. It never
  initializes replacement state over a failed home.
- Recovery constructs the candidate from a fresh Fjall configuration and retains no dependency
  cache from the failed generation. Block and blob residents are live-generation performance state,
  never reopen-validation evidence or authority for the replacement generation.
- A recovered private candidate must validate the home header, registry, required schema
  declarations, and physical families, reacquire every registered domain through fresh typed
  handles, and complete the required Fjall physical verification. These structural checks do not
  create a schema-validation boundary or exhaustively scan application records during routine
  recovery. Any disagreement or I/O failure discards the candidate and permits a later retry.
- Successful forced reopen assigns the private candidate a new monotonic process-local home
  generation and store-instance identity, but exposes no app-usable healthy handle. The system
  recovery boundary consumes the candidate once and alone authorizes full-stack publication.
  Handles, commands, sidecar tokens, receipts, and asynchronous completions from the obsolete
  generation cannot authorize candidate work or later publication.
- The package exposes the accepted recovery delays as `1`, `2`, `5`, `10`, and `30` seconds, remaining at `30` seconds until successful recovery resets the schedule. Scheduling and preserving caller-owned in-memory GUI values remain application responsibilities.
- A health failure rejects new commands according to the system gate without closing application
  windows, mutating caller-owned coherent values, reading CAS, or inventing fallback data. The
  package never returns volatile CAS-interrupt authority; it can return only exact pre-writer
  rejection evidence or `NotCommitted` proof for a command that performed no durable mutation.
- Rebuildable domain projections may be invalidated and rebuilt only when their domain contract permits it.
- Whole-home scrub is a separate bounded-memory explicit or background operation, or is invoked by
  evidence of corruption or a schema-validation boundary. It is not the routine recovery gate for
  an ambiguous mutation.
- Exactly one whole-home scrub worker may run per home. Concurrent requests join that result and
  evidence arriving mid-run may coalesce into at most one pending rerun. Every terminal path drops
  the snapshot, cursor pages, sidecar verifier state, and worker permit before another scrub begins;
  this local cap is independent of writer and reconciliation permits and is not a universal
  governor.

## Physical Theme Repository

- This package implements the physical installed-theme repository boundary assigned by the
  [Beryl-home storage system](../../../doc/systems/beryl-home-storage/design.md). It supplies bounded
  range reads, staged atomic document replacements, exact length and digest verification, file and
  directory durability, atomic manifest replacement, and bounded change notifications to the typed
  `beryl-state` theme service.
- The physical API accepts package-neutral file identities, byte ranges, exact expected file and
  manifest identities, bounded staged byte streams, and explicit per-operation limits. It does not
  parse theme documents, assign installed-theme ids or order, interpret settings, resolve appearance,
  or expose raw file handles and paths to callers.
- The physical layout is `<beryl-home>/themes/manifest.toml` plus user-editable stable documents at
  `<beryl-home>/themes/installed/<stable-theme-id>.toml`. The typed caller supplies an already-
  validated package-neutral file identity; this package derives the relative filename and never
  accepts an arbitrary relative or absolute path.
- `manifest.toml` is Beryl-owned membership and ordering authority. Installed document files are
  supported external-edit inputs, while a document absent from the manifest remains inert. A root-
  level `theme.toml` is never part of this package's repository boundary.
- Publication flushes every required staged file before its authoritative atomic replacement and
  applies the home filesystem tier's directory-durability rules. A newly installed document is inert
  until selected by the durable manifest; an already installed document's stable file is its direct
  editable content source. Readers observe one complete old or new manifest generation and only
  complete atomically replaced document files.
- Beryl-authored document updates flush a sibling staged file and atomically replace the stable
  installed TOML path. Membership, name, and order mutations use manifest-last publication; delete
  removes manifest authority before any non-authoritative file removal, and a retained file remains
  inert.
- The terminal physical result proves non-publication, exact durable publication, or indeterminate
  owner-manifest publication and carries only the bounded file evidence needed by the caller's exact
  natural-record reconciliation hook. It cannot fabricate a repository generation or authorize a
  retry, rollback, parse, or appearance publication.
- Temporary or unreferenced staged files remain inert after cancellation, failure, or process exit.
  Ordinary open and targeted reconciliation do not guess that those files are authoritative or safe
  to delete.
- One bounded coalescing repository watcher reports package-neutral changed-file hints, manifest
  change, and overflow to the typed caller. Signals contain no path or bytes and are never commit
  evidence; overflow requests one bounded coherent refresh. Watcher shutdown, store failure, and
  same-home recovery release the old generation's subscription and queue rather than adopting it.

## Sidecar Publication

- Sidecars live under `sidecars/<namespace>/<first-two-SHA-256-hex>/<full-SHA-256-hex>`. Typed durable metadata owns the namespace, digest, and exact byte length; every admission and verification also requires an explicit nonzero caller byte limit.
- Admission creates the sidecar directories needed for the digest path. On the fully supported local
  NTFS tier it synchronizes each newly published directory link before dependent metadata may
  commit; best-effort tiers perform the strongest available sequence.
- Admission writes a unique temporary file, flushes its complete content, and renames it to the
  digest path without replacement where supported. Fresh publication, existing reuse, and a
  concurrent no-replacement winner converge by verifying the final file's exact length and SHA-256
  digest. Existing reuse or a concurrent winner also compares that final file byte-for-byte with the
  exact staged source through caller-bounded pages before returning an admission token; any differing
  byte is a collision invariant failure. The containing directory is synchronized on the fully
  supported tier before the admission token is returned.
- The returned `AdmittedSidecar` is valid only for its healthy store generation. A metadata command
  that first references those bytes retains this token through its batch and `SyncAll` barrier; a
  failed or obsolete token cannot authorize metadata publication.
- Registered domains may use the bounded `SidecarVerifier` to prove that typed references name
  final files with the declared length and digest.
- Failed admission may leave an inert temporary file or an unreferenced final file. The package never deletes either form and exposes no cleanup operation before the future home-wide garbage-collection design.

## Fault-Test Boundary

- The `test-faults` Cargo feature exposes deterministic actions only at concrete package call
  boundaries around reads, batch commit, persistence, verification, forced reopen, sidecar file and
  directory operations, and theme staged-write, flush, replacement, removal, directory-sync,
  observation, and watcher operations. Production builds compile those checks to no-ops; there is
  no alternate storage engine, virtual filesystem, compatibility layer, or retry path.
- Writer actions may additionally be scoped to the exact Rust mutation type carried by a typed
  current-domain command. The scope is process-local test identity, remains at the same physical
  before-commit, after-commit-before-persist, and after-persist boundaries, and only prevents an
  unrelated typed command from consuming an intended action. Unscoped actions retain their prior
  behavior, and neither scope metadata nor scope selection exists in production builds.
- The feature additionally exposes one bounded persisted-corruption seam for post-registration read-health, verification, and recovery proofs. It requires the exact current typed domain handle and codec owner, accepts only a nonempty physical record envelope that the registered exact codec rejects, shares the existing explicit same-thread writer-reentry guard, serializes through the existing writer, and completes `SyncAll`.
- The corruption seam enforces fixed fixture-byte ceilings, exposes neither Fjall handles nor a reusable raw reader/writer, and rejects every envelope the exact codec would accept. It is absent from production builds and cannot bypass registration, recovery, or validation there.
- The feature also exposes one no-input maintenance-terminal fixture. It installs an actual retained
  terminal through Fjall's non-production fault boundary while deliberately leaving the Beryl gate
  healthy, exposes no database handle or generic engine operation, and exists only to prove that
  every state-dependent result observes dependency health before publication.
- Package tests inject surfaced errors with exact I/O kinds, typed sidecar write/rename/sync
  barriers, all four reserve-query outcomes, ordinary `ENOSPC`, deterministic concurrency blocks, writer
  panics, subprocess aborts, parent-forced termination, callback-stage failures, closed-generation
  raw corruption, and bounded post-registration exact-codec-rejected envelopes. They prove exact
  owner and codec identity, bounded command work, writer-reentry rejection, whole-home scrub
  behavior, ordinary-read fail-closed classification, stale-read publication rejection, all three
  command-result payload contracts, exact-old and receipt-reconstructing exact-new reconciliation,
  mixed-and-neither collision closure, and obsolete-generation rejection. Capacity tests fill all
  1,024 retained scopes and the aggregate byte budget independently, prove the next potentially
  indeterminate mutation returns pre-writer `NotCommitted` without disturbing admitted work,
  reject a descriptor above 64 MiB, join duplicate
  triggers without duplicate queue work, and prove immediate reservation release for directly
  classified `NotCommitted` and `Committed` plus gate and slot release for `ExactOld` and
  `ExactNew`. Collision tests prove opaque-descriptor disposal, one bounded sealed-fact set and its
  closed slot remain, and every transient worker resource is released. Open tests prove that no
  candidate application discovery is available before complete typed-stack publication.
- Durable-start footprint tests independently compose both allowed typed participant pairs, prove
  the direct logical total is 26 records and 1,263,194 encoded key-plus-value bytes, prove the queued
  logical envelope is 25 records and 1,328,212 bytes, and prove owned journal framing raises the
  queued shared maximum to 1,328,763 bytes. They reject mismatched participant kinds and checked-
  arithmetic overflow; no test substitutes an arbitrary aggregate input.
- Reserve-query tests prove only the validated requirement type is accepted, every invocation makes
  one filesystem observation, and `Sufficient` performs no reservation or later-`ENOSPC` guarantee.
- Custody tests prove that an `Indeterminate` result is move-only, registry acceptance cannot fail
  after reservation, explicit and drop-fallback installation retain the exact descriptor and charge,
  route cancellation and service retirement cannot drop the descriptor, and orderly close or
  ordinary store/error destruction cannot dispose the registry or unlock a home with retained scope.
- Package tests also cover four-worker saturation and release, filesystem-tier behavior, and
  sidecar publication ordering at the boundaries Beryl controls.
- The owned Fjall fork exposes a deterministic non-production journal-write failure seam. Package
  tests exercise that exact failure before in-memory batch publication and prove it cannot be
  followed by reported durable success; production builds expose neither the seam nor raw Fjall
  mutation authority.

## Dependency Boundary

- This package depends on the unpublishable owned Fjall fork at `../fjall-fork` and may depend on
  platform file-lock primitives.
- The Fjall fork must require bounded block, value, cursor-topology, cache, memtable, and batch
  policy, expose metadata-first stored-value inspection, reject configured limit violations, and
  propagate journal write failures. Its independently versioned `lsm-tree` fork owns the lower
  storage-engine implementation.
- It must not depend on `gpui`, `beryl-app`, `beryl-backend`, or CAS protocol types.
- `syndic-storage` and Beryl metadata packages consume or register through this boundary without depending on one another's private records.
