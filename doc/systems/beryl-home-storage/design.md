# Goals

Define one durable Beryl-home storage system that keeps Syndic conversation state and Beryl application metadata physically efficient while preserving strict logical ownership, typed access, atomic mutation, and crash recovery.

Provide the process lock, session bootstrap, runtime/root registry, thread catalog projection, window claims, settings records, durable jobs, and failure gates needed by Beryl's features without exposing Fjall internals across package boundaries.

## Non-goals

- Allowing concurrent ownership of one Beryl home by multiple OS processes.
- Sharing one Syndic store between different Beryl homes.
- Importing workspace-era state or providing compatibility reads, dual writes, or migration adapters.
- Making the database, raw keyspaces, encodings, or Fjall transactions available to GUI or backend code.
- Storing CAS authentication, Codex configuration, app-server capability tokens, or backend-owned conversation history.
- Maintaining a second session snapshot outside Fjall.
- Detecting or surviving external replacement, rollback, or tampering inside an Operator-selected
  Beryl home while Beryl is not the actor performing those changes.

# Decisions

## Participating Boundaries

- `doc/systems/bounded-resource-dataflow/design.md` owns the risk-based rules for large durable
  reads, result pages, staging, queues, caches, and scans. Storage packages use shared
  `beryl-stream` paging and bounded-channel primitives where useful, but no process-wide allocation
  capability is required.
- `beryl-home-store` owns the physical database supplied by the unpublishable owned
  Fjall fork, process-wide writer, state-store health, home lock, typed domain registration
  boundary, and physical installed-theme repository operations.
- Opening that store supplies explicit cache, memtable, stored-record, decoded-result, page, batch,
  and storage-concurrency limits. These are practical store configuration, not exact reservations
  against a universal process-memory account.
- `beryl-state` owns the typed Beryl runtime/root, session/window, claim, settings, durable-job,
  catalog, and asset-metadata schemas and command contributors registered through that physical
  boundary. It also owns the typed repository/parser/resolver service assigned by the
  [theme runtime system](../theme-runtime/design.md), while this system retains physical theme-file
  access and durability. It owns no canonical per-thread metadata domain.
- `syndic-storage` owns typed Syndic records, codecs, queries, and mutations within its assigned keyspace family. It does not open a second database or expose raw Fjall handles.
- `beryl-app` consumes typed Beryl-home and Syndic APIs for shell orchestration. It does not own on-disk encodings or direct database access.
- `beryl-model` owns only pure identities and values shared across packages.
- `beryl-backend` does not access the Beryl-home store.
- Cross-domain atomic operations are expressed as typed home-store commands whose participants contribute validated mutations to one physical commit.

## Physical Home Layout

- One Beryl home owns exactly one Fjall database for durable Syndic records and Beryl application metadata.
- After the home lock is acquired and before Fjall opens, Beryl rejects an existing `state`
  symlink, junction, file, or other reparse-point collision, then creates or opens the ordinary
  `state` directory and validates the expected home header and schema. The Beryl home is trusted
  storage; Beryl does not retain filesystem-object identity or deny external replacement for the
  store lifetime.
- The home also owns bounded sidecar directories for large image and Syndic resource payloads and
  the physical file-based installed-theme repository.
- Image bytes are ordinary content-addressed sidecar files under the Beryl home, never Fjall values or blob payloads. Fjall stores their typed metadata, durable references, and sidecar state.
- Scalar application settings, runtime/root configuration, window/session state, durable
  orchestration jobs, catalog projections, and asset references live in Beryl-owned Fjall keyspaces.
  Intrinsic thread properties live in Syndic keyspaces in the same physical home.
- Installed theme documents and order remain in that physical repository; the active theme identity
  and other scalar theme settings live in Beryl Settings records.
- Physical theme access supplies bounded range reads, staged immutable generation files, exact
  length/digest verification, file and directory durability, and an atomic owner-manifest
  publication step to the `beryl-state` theme service. New document files become authoritative only
  through the durable owner manifest, so readers observe one complete old or new repository
  generation.
- The physical boundary reports proven non-publication, exact durable publication, or indeterminate
  owner-manifest publication with the exact file evidence needed for targeted reread. It does not
  parse compact TOML, assign theme ids, interpret installed order, resolve roles, arbitrate preview,
  or publish appearance.
- Within one process, canonicalized configured paths must not select the same home twice.
- Every stored record includes a schema version appropriate to its logical domain. An unsupported
  version fails the ordinary read that decodes it or an exhaustive validation boundary with a typed
  error rather than being guessed or rewritten opportunistically; routine structural open alone
  does not claim to discover a dormant record.

## Keyspace Ownership

- Syndic keyspaces own threads, immutable thread execution bindings, intrinsic thread attributes and
  usage observations, current drafts, submitted turns, canonical items, transcript views,
  projections, resources, source events, provider identities, CAS projection bindings, compact
  thread summaries, and Syndic revisions.
- Beryl runtime keyspaces own runtime records and configured-root records.
- One Beryl session domain owns the current restore-set generation, durable window records, selected thread ids, geometry, virtual-desktop identity, window-local remembered runtime/root values, exclusive main-window claims, and the last successfully used runtime/root fallback retained when the restore set becomes empty.
- Session, window, and both claim-index families share one typed domain so exhaustive domain
  validation can prove their forward and reverse invariants without a cross-domain raw-storage
  escape hatch.
- Beryl job keyspaces own durable resolution-handoff and other explicitly designed host-orchestration jobs.
- Beryl catalog keyspaces own compact rebuildable catalog rows and deterministic recency indexes
  derived from Syndic thread summaries plus Beryl runtime/root availability and claim facts.
- Beryl settings keyspaces own validated app-wide scalar preferences. Feature-specific schemas remain owned by their feature and settings contracts.
- Beryl asset keyspaces own asset metadata, immutable paged reference sets, and compact owner heads.
  Asset bytes remain sidecars addressed through typed asset identities.
- No domain may scan, decode, mutate, or infer another domain's private records. Cross-domain joins and commits use explicit typed operations.

## Typed Domain Authority And Validation

- Every live logical-domain blueprint, handle, and command contribution is bound to its exact Rust owner type in addition to the durable stable name and schema. Every declared record family is bound to one exact codec type. These process-local identities are not persisted, and a same-name/schema type or alternate same-name codec cannot reacquire another live owner's authority.
- Routine open, reopen, and handle reacquisition validate those exact live types, durable
  declarations, required families, and generation without scanning every application record.
- Every mutation domain also registers a separate bounded natural-record reconciliation hook. That
  hook can read only the exact operation identities admitted by the command's pre-writer
  descriptor reservation and materialized under the serialized writer before physical mutation;
  it cannot be substituted by the domain's exhaustive validator.
- Only registration at a schema-validation boundary, an explicit whole-home scrub, background
  maintenance, or corruption-evidence investigation may use the store-owned exhaustive validation
  path. It streams
  every snapshot-current application record in
  every physical family with bounded memory, checks physical key/value bounds, applies the exact
  owning codec to every key and versioned value envelope, and only then runs the domain's
  cross-record and sidecar invariants. "Physical record" here means a current stored application
  envelope, including one outside every typed query range; it does not expose retired LSM versions
  or tombstones as application records.
- Cursor-only sentinel keys may define finite typed query bounds but are illegal stored identities. Unknown, malformed, oversized, sentinel, and otherwise out-of-range raw keys fail exhaustive validation even when they lie outside every ordinary typed cursor range.
- Domain callbacks return storage-owned `ReadError` and `SidecarError` provenance through an explicit typed channel. Semantic domain rejection remains separate; the store never classifies health by searching an arbitrary erased error chain.
- Each registered codec declares the maximum stored key/value shapes it accepts and decodes under
  caller-supplied item and decoded-byte page limits. Point and cursor APIs reject malformed or
  oversized records explicitly and return ordinary typed bounded results.
- The owned Fjall boundary supplies configured block, separated-value, merge-source, retained-
  topology, cache, memtable, and batch limits plus metadata-first reads. Beryl neither reconstructs
  dependency allocation formulas nor requires exact database or operation residency quotes.
- Exhaustive work runs away from the serialized writer and GPUI thread. It may take total work
  proportional to durable domain size but retains only bounded records and state at once.

## Home Ownership Lock

- Before opening Fjall, Beryl creates or opens one fixed lock file inside the requested home and attempts a non-blocking exclusive OS file lock.
- The process retains the locked file handle until orderly shutdown completes. Normal release is explicit; handle release on process death is the crash fallback.
- Lock-file existence is not ownership evidence. A file left after a crash is reusable when its OS lock is no longer held, so no stale-lock deletion protocol is required.
- Opening requires a reliable exclusive lock for the complete home lifetime. If the selected
  filesystem cannot provide that lock, opening fails.
- Beryl canonicalizes the configured home path for in-process identity. User-facing paths may
  preserve their configured spelling, but ownership and registry keys use the canonical path.
- Native local NTFS homes receive the full crash-durability contract in this system. Other
  filesystems, including UNC, WSL-backed, removable, and synchronized locations, may open only when
  basic access and the reliable exclusive lock work. They receive best-effort durability and a
  feature-owned timed warning; Beryl does not attempt to prove remote or synchronized durability
  conditionally at open time.
- Diagnostic lock contents may include bounded process identity, but Beryl never trusts file contents as proof that the owner is alive and never breaks a live OS lock.

## Serialized Writer And Revision Checks

- One process-wide home-store writer serializes every Fjall mutation while bounded point and cursor reads may execute concurrently through typed read APIs.
- Serializing commits is an internal atomicity mechanism, not a durable database lock and not a restriction on simultaneous CAS turns for different Syndic threads.
- Every correctness-sensitive mutation carries the expected revisions of the records it read, including thread, draft, window claim, session, job, or binding revisions as applicable.
- A typed operation that mutates exactly one domain from exact logical record revisions may use a
  writer-admitted current-domain command. That opaque command captures only the physical home and
  domain revisions after it owns the serialized writer; it still validates every operation-owned
  logical revision, never retries semantic rejection, and cannot carry another domain or a
  sidecar. Unrelated commits therefore cannot make a live-event or projection command stale while
  it waits for writer admission.
- Caller-sampled physical revisions remain required for heterogeneous cross-domain and
  sidecar-retaining commands because their participants must share one explicitly prepared atomic
  basis.
- The writer validates all expected revisions, exact participant owner identities, and persistent registration declarations immediately before assembling the commit. It then runs only operation-bounded contributor validation and assembly; it never performs a whole-domain scan. A mismatch rejects the whole command with a typed conflict; it never merges opaque payloads or creates a competing same-thread child.
- One explicitly typed validation-only domain participant may join a command that contains at least
  one mutation participant. It runs against the same writer-time snapshot and caller-sampled domain
  revision, but has no mutation builder, cannot retain a sidecar, advances neither home nor domain
  revision by itself, and contributes no affected-domain revision to the receipt. Duplicate
  participation by the same typed domain remains invalid across both roles. An ordinary mutation
  that emits no changes remains an error, so validation intent cannot hide an accidental empty
  write path.
- One accepted command writes every participating keyspace mutation in one Fjall write batch. A
  successful Fjall `SyncAll` outcome means that complete batch committed atomically and is durable;
  only then may Beryl report durable success.
- The Beryl command outcome is classified against that complete durability boundary. A Fjall
  `Committed` error from the preceding buffered journal step remains exactly retained in the typed
  failure but maps to Beryl `Indeterminate` with a reconciliation descriptor when `SyncAll` did not
  subsequently succeed; it never fabricates a durable receipt. Only a failure observed after
  successful `SyncAll` may produce `Committed { receipt, later_failure }`.
- Cancellation is authoritative only before writer admission. Once an operation enters the serialized writer, callers drain its outcome and never interpret a later cancellation request as proof that the batch did not commit.
- The home-store command result has exactly three variants: `NotCommitted { evidence }`, whose
  typed evidence proves that no part of the batch committed and which carries no receipt or
  reconciliation descriptor; `Committed { receipt, later_failure }`, which always carries the
  exact durable command receipt and an optional typed failure observed after commit; and
  `Indeterminate { failure, reconciliation }`, which carries the surfaced typed failure and one
  move-only custody value containing the sole opaque operation-scoped reconciliation descriptor
  together with its already-reserved registry slot and byte charge. It carries no receipt and
  authorizes no publication.
- Each mutation domain contributes a typed natural-record reconciliation hook when it builds an
  operation that could become indeterminate. The opaque descriptor retains only the exact old
  natural-record identities, revisions, and values needed by those hooks plus the intended exact
  new state and receipt facts; it is not a raw keyspace reader or general recovery capability.
- Targeted reconciliation returns exactly `ExactOld`, `ExactNew` with the reconstructed exact
  durable receipt, or `Collision`. `ExactOld` proves the operation did not become authoritative.
  `ExactNew` proves the complete intended atomic state and reconstructs the same receipt a direct
  `Committed` result would have carried. Any mixed old/new state or state matching neither exact
  side is `Collision`.
- An indeterminate result closes publication and further admission only for its exact operation
  scope. `Collision` leaves that scope closed; it never guesses, merges records, invokes a
  whole-home scrub, clears or crosses the old writer, or turns CAS into storage authority.
  Structurally healthy unrelated work remains admitted. Only separately observed corruption,
  invalid schema or registry state, poisoned current authority, or another structural failure may
  fail the complete store lifecycle.
- `beryl-home-store` owns one process-local reconciliation-scope registry for each open home. Its
  lifetime begins with home ownership and ends only after final home close; it outlives individual
  Fjall generations, failed services, unpublished recovery candidates, brokers, connections, and
  CAS-live services. The registry has exactly 1,024 slots, a 64-MiB encoded-byte ceiling for each
  potential descriptor, and a 256-MiB aggregate retained descriptor-budget ceiling. Before writer
  admission, every mutation that could become indeterminate obtains one move-only reservation and
  proves from its
  command-owned identities plus declared schema limits that its conservative descriptor-byte
  budget fits that ceiling. After writer admission, the command materializes the exact old state,
  intended new state, and intended receipt facts from the admitted snapshot into that reserved
  budget before batch construction or any Fjall mutation. Scope saturation returns
  `NotCommitted { evidence: ReconciliationCapacity }`; an oversized descriptor returns the
  exact `NotCommitted { evidence: ReconciliationDescriptorTooLarge }` result. Neither rejection
  enters writer admission, creates a scope, or changes structural health.
- Orderly final home close first stops new reservations and drains every admitted command. It cannot
  dispose the registry while a verifying scope still owns a descriptor: close joins its already
  admitted classification work or remains failed and leaves the home open. After no descriptor-
  bearing scope remains, final home close may dispose collision-sealed process-local facts together
  with the registry; the exact durable natural records remain authoritative on the next process
  open. Forced process termination publishes no in-memory result or acknowledgement.
- A directly classified `NotCommitted` or `Committed` command releases its reservation with its
  writer-time state. `Indeterminate` first transfers the reservation and sole descriptor into its
  move-only custody value. The immediate recipient must synchronously consume that value into the
  already-reserved exact registry gate before it translates or erases the result, installs an
  acknowledgement, releases operation state, or observes route cancellation. Registry acceptance
  is infallible because capacity was reserved before writer admission. Once accepted, the registry
  is the unique owner; no other result, acknowledgement, broker, or service retains a copy.
- Provider ingress uses the same rule: the app-owned `Ingester` performs the registry handoff before
  completing `BrokerReply`, closing `AckSlot`, or releasing `ActiveObservation`. The acknowledgement
  may report that publication did not occur and reconciliation is required, but it carries neither
  a receipt nor the descriptor. Connection loss, target retirement, caller cancellation, and
  service failure cannot discard or retract registry-owned custody.
- Installing custody closes the exact publication scope but authorizes no reread, retry, rollback,
  publication, or reconciliation execution. A separately admitted targeted-reconciliation trigger
  may later consume that retained scope under the worker and natural-record-hook rules below.
- Registry installation is also the lifetime cut for process-local command continuations. The
  caller may release its old operation or stager after handoff; a domain hook reconstructs any
  `ExactNew` successor, permitted same-live-owner `ExactOld` continuation, or terminal `Collision`
  disposition solely from descriptor-bound natural records. No failed service, connection, broker,
  acknowledgement, or process object must survive until classification.
- Every `Committed` result carries an opaque exact receipt with the process-local home generation,
  committed home revision, and revisions of only the participating domains. Domain owners project
  their own affected revision from that receipt; an unaffected domain yields no revision.
- Publication based on a successful asynchronous completion must validate its receipt against the exact current healthy generation and private store instance. Equal durable revision numbers do not make a prior-generation or foreign receipt current.
- A command that spans Syndic and Beryl domains is all-or-nothing at the typed home-store boundary.
  Partial logical success is not published even if later background work such as CAS delivery has
  not yet occurred.
- Callers cannot hold a transaction or writer guard across asynchronous CAS, filesystem, windowing, or model work. External work uses prepare, short revision-checked commit, execute, and short result-commit steps with durable intent records where recovery is required.
- Exactly one command may hold writer admission. The store owns no unbounded writer queue: callers
  await the single admission permit with backpressure, may cancel only while waiting, and release
  the permit only after the command has produced its classified result and dropped all writer-time
  batch and callback state.

## Durability Classes

- User input admission, draft flush, thread/draft creation, thread claim, runtime/root creation,
  generated-title or automatic-archive thread-attribute mutation, session restore-set mutation,
  resolution intent, handoff-job transition, and CAS-binding transition require `SyncAll` before
  the caller may report them saved or accepted.
- A lifecycle flush waits for any already-admitted correctness-sensitive command and then reconciles or persists the latest exact state. It does not attempt to cancel admitted work or let close, Exit, switching, or submission continue from an ambiguous outcome.
- Each admitted provider-capable backend connection retains one reference to its exact home
  generation for pre-route Syndic staging. Application shutdown first retires and joins every such
  ordered-ingress broker and backend connection, then releases those references before explicitly
  closing the sole owning `HomeStore`. A live connection never races an already-started explicit
  close or turns it into an unreported drop-only unlock.
- Live assistant deltas and other replayable stream updates may be coalesced into bounded short commits, but every published commit preserves a valid incomplete turn and uses `SyncAll`. A crash may lose only the uncommitted stream suffix.
- High-frequency window geometry and transcript-position state may be coalesced by durable key, but explicit Exit, ordinary window close, thread switch, and application shutdown create flush barriers.
- Rebuildable catalog indexes may lag their source records only when a durable stale marker makes that state explicit. Catalog publication waits for a coherent rebuilt durable index generation rather than mixing source and stale index revisions or materializing all rows.
- No caller may request weaker persistence for a correctness-sensitive class merely to reduce latency. A later design decision is required to weaken one of these guarantees.

## Turn-Start Free-Space Admission

- The home-store query reports exactly `FreeSpaceOutcome::Sufficient`,
  `FreeSpaceOutcome::BelowReserve`, `FreeSpaceOutcome::Unavailable`, or
  `FreeSpaceOutcome::Indeterminate`. The latter two distinguish a proven inability to query from a
  query whose platform result cannot be trusted; neither is treated as sufficient.
- Every direct or queued new-turn start invokes the query exactly once, synchronously and
  immediately before its durable turn-admission command. Beryl does not poll free space, run a
  timer, cache the result, or apply hysteresis. Steering an already active turn is not a new-turn
  admission and performs no reserve query.
- Every outcome except `FreeSpaceOutcome::Sufficient` denies that admission before writer
  admission, preserves the exact input for a later attempt, and dispatches zero CAS work.
  `FreeSpaceOutcome::Sufficient` permits the ordinary revision-checked admission attempt but
  reserves no bytes.
- The reserve check is an early admission guard, not a promise that later writes cannot encounter
  `ENOSPC`. Ordinary storage-error and ambiguous-outcome handling still applies to every write.

## Sidecar Commit Ordering

- Large asset and Syndic resource bytes may live in content-addressed sidecar files under the Beryl home.
- On the fully supported local NTFS tier, sidecar publication writes a unique temporary file,
  flushes its complete bytes, atomically renames it without replacement to the digest-derived final
  path, and synchronizes the containing directory before durable metadata may reference the bytes.
- Fresh publication, existing-file reuse, and a concurrent no-replacement winner converge by
  verifying the final file's exact length and digest. Reuse of an existing file or concurrent
  winner additionally requires one bounded page-by-page byte-for-byte comparison with the exact
  staged source before an admission token can publish; a differing byte is a collision invariant
  failure. Metadata is committed only after those bytes are present and verified. Best-effort
  filesystem tiers perform the same ordering with the
  strongest available rename and directory-sync operations, including no-replacement rename where
  supported, but do not gain the full durability promise.
- A committed asset owner head must never select a set entry whose sidecar was not fully written,
  renamed, and represented by matching durable metadata.
- A crash may leave an unreferenced completed sidecar or temporary file. Such files remain inert until the future explicit garbage-collection design; ordinary startup does not guess that they are safe to delete.
- No sidecar bytes are deleted before the future explicit garbage-collection design, including after reference removal or when bytes appear unreachable.

## Store Health State

- Structural store lifecycle is exactly `opening`, `healthy`, `failed`, or `reopening`. Exact
  operation-scoped publication gates separately move through `open`, `verifying`, and `closed`;
  scoped verification is never a structural store lifecycle state.
- Acceptance of a surfaced `Indeterminate` custody value rejects publication and moves only its
  affected gate from `open` to `verifying`. `ExactOld` or `ExactNew` reopens that gate; `Collision` moves it to
  `closed`. Malformed durable records, invalid trusted codec or registration contracts, poisoned
  current authority, missing required keyspaces, and sidecar invariant violations separately move
  the structural store lifecycle to `failed`. Semantic mutation rejection leaves both lifecycle
  and unrelated gates unchanged.
- Every state-dependent result passes one store-owned publication confirmation that first observes
  the exact admitted Fjall database's retained autonomous-maintenance terminal and then confirms
  the Beryl health generation. This includes reads, writes, domain registration and reacquisition,
  command-receipt revision projection, and sidecar admission and verification; no result path may
  invoke only the cached Beryl gate check.
- A panic unwinding through an admitted writer fails the store closed immediately. Recovery does not
  reuse that poisoned writer; it creates a fresh store service and fresh writer after the physical
  database and required schema are reopened successfully.
- Ambiguous-outcome verification is targeted: it drains affected admission, verifies or reopens the
  physical Fjall database as needed, and invokes only the descriptor's domain-owned natural-record
  hooks. A physical or hook error keeps that gate closed unless its independent evidence also
  proves a structural store failure. `Collision` never escalates merely because it is unresolved.
- The registry's 1,024 slots and 256-MiB aggregate byte budget bound the total of pre-writer
  reservations, verifying scopes, and collision-closed scopes, not only active workers. Each
  verifying scope retains exactly one opaque
  descriptor; each collision-closed scope retains only its bounded sealed facts. Another trigger
  for the same retained scope joins its existing result and never adds a descriptor, sealed-fact
  copy, or queue item.
- One home runs at most four targeted reconciliation workers and at most one worker for any exact
  scope. When all four worker permits are held, a registered scope remains gated and awaits a
  permit without duplicating its descriptor.
- `ExactOld` and `ExactNew` remove the scope and release its registry slot, complete retained byte
  charge, descriptor, worker permit, snapshot, reader, pages, and hook state. `Collision` compacts
  its evidence into only the configured-byte-bounded sealed old/new identities, revisions, digests,
  and collision facts. In the same registry transition it replaces the descriptor's conservative
  retained-byte charge with the sealed facts' exact encoded-byte charge, discards the descriptor,
  retains that closed scope, slot, and replacement charge, and releases the worker permit plus every
  transient reader, snapshot, page, and hook allocation. The sealed-fact schema maximum is included
  in the pre-writer reservation, so this transition never needs new registry capacity. A typed
  reconciliation failure likewise releases all transient worker state while leaving at most the one
  bounded descriptor and its original retained-byte charge in the gated scope.
- Registry or descriptor-capacity saturation fails the new mutation closed through its typed
  `NotCommitted` result without failing the structural store. Unrelated already-admitted healthy
  work continues, and later unrelated work may proceed whenever it needs no unavailable scope
  slot.
- While an affected gate is `verifying`, related commands and reads that could publish dependent
  state are gated. Unrelated healthy work and already coherent in-memory windows remain intact
  unless the evidence requires store-wide failure.
- `failed` closes the shared Fjall admission gate for every operation identified by `doc/features/beryl-home/design.md` and retains each current window's identity, placement, selected thread, and last coherent resident surface in memory. The store does not fall back to CAS or cached projections as durable authority.
- Store failure does not itself authorize or dispatch a volatile CAS interruption. Storage exposes
  only exact evidence that a command was rejected before writer admission or returned
  `NotCommitted`; that evidence may prove that preserved new-turn input had zero CAS dispatch, but
  it grants no CAS command capability. Every other affected turn remains for the CAS-live outage
  and repair boundary after a fresh service exists.
- Failure observation closes the affected publication and command-admission gates. Ordinary
  shutdown may join that quiescence, but storage recovery retains no live CAS connection,
  projection, quarantine, service epoch, or adoption capability.
- CAS events, command results, and other incoming work observed while the Fjall gate is closed cannot be committed or published as durable Syndic state. The last successfully committed lifecycle record remains the recovery starting point.
- Recovery drains and drops the failed Fjall generation, constructs a fresh Fjall configuration,
  reopens `state`, validates the home header, registry, and required schema, and reacquires fresh
  typed handles into an unpublished private home-store candidate behind the startup fence. It does
  not require opened-object identity continuity or retain dependency handles from the failed
  generation.
- Each Fjall database generation owns a fresh dependency cache. Recovery drops the failed
  generation's cache and constructs the candidate with a new Fjall configuration; no block or blob
  resident crosses reopen or contributes evidence to reopen validation. Cache reuse is confined to
  trees and immutable versions inside one live database generation.
- A failed recovery leaves the process in `failed`; it never initializes an empty replacement database at the same path.
- Running-session recovery uses one process-wide single-flight loop with delays of 1, 2, 5, 10, and 30 seconds, then repeats at 30 seconds while failure persists.
- A new failure signal while recovery is active joins that attempt instead of creating another
  loop. Successful physical reopen alone does not cancel retries or publish application state.
- Behind the startup fence, the system reacquires the complete Beryl and Syndic typed handle set,
  runs every pending operation descriptor to `ExactOld`, `ExactNew`, or scoped-closed `Collision`,
  installs those scope gates, and constructs a fresh CAS-live service. That service repairs each
  eligible affected turn from its exact durable natural identities. Storage supplies fresh typed
  reads and commits but never retains, selects, resumes, adopts, or blesses a CAS capability.
- A repair-required turn may consume at most one exact correlated terminal CAS turn snapshot and
  commit the result through Syndic; storage does not globally prohibit that bounded repair read or
  treat CAS as canonical history.
- Only after that system recovery sequence completes does one startup-fence transition publish the
  app-visible home-store, typed-state, Syndic, and CAS-live stack together as the new healthy
  generation. A scope classified `Collision` remains closed in that published stack while
  unrelated structurally healthy scopes remain usable. Publication cancels later recovery retries
  and invalidates every prior-generation typed handle, prepared command, sidecar token, command
  receipt, and asynchronous completion.

## Whole-Home Validation

- A bounded-memory whole-home scrub runs only at a schema-validation boundary, as an explicit
  requested operation, in background maintenance, or upon corruption evidence. Routine recovery
  and ambiguous-outcome reconciliation never invoke it.
- Exactly one whole-home scrub worker may run for one home. Concurrent requests join its result;
  evidence arriving during the run may set one coalesced pending rerun, never an unbounded work
  queue. Every terminal path releases the worker, snapshot, cursor pages, sidecar verifier state,
  and pending-rerun flag before another scrub starts. This local limit is independent of the writer
  and reconciliation limits and does not create a universal process governor.
- During that scrub, store-owned validation visits every snapshot-current
  application key and value in every registered physical family through its exact codec. Typed
  cursor endpoints cannot hide malformed records outside an ordinary query range; retired LSM
  history and tombstones remain storage-engine internals rather than codec inputs.
- Whole-home validation checks the active session generation, window-record references, thread/draft
  uniqueness, committed-tail reachability, monotonic revisions, exclusive thread claims, CAS
  binding reverse uniqueness, pending-job state, catalog stale markers, asset owner-head to sealed
  reference-set agreement, and metadata-to-sidecar references.
- Rebuildable catalog projections may be marked stale and rebuilt from validated source records.
- Missing or corrupt authoritative thread, draft, session, job, binding, or asset-reference records fail validation rather than being silently dropped.
- Incomplete live work remains explicit through its owning lifecycle records. When those records
  cannot prove one turn's terminal state, only the bounded delegated terminal-turn repair above may
  consult CAS.

## Runtime And Root Registry

- A runtime record has a stable Beryl id, one canonical absolute Codex CLI executable path, its derived Host or exact WSL-distribution mode, its exact runtime-native executable path, bounded availability facts, and a revision.
- Runtime and root ids are allocated by the admitting orchestration boundary before the short storage command begins. `beryl-state` validates and persists those already admitted identities; it does not probe filesystems, launch CAS, read a clock, or generate ids while holding writer admission.
- Canonical executable identity is unique within one Beryl home. Selecting an already registered executable resolves to the existing runtime record rather than creating a duplicate.
- A configured-root record belongs to one runtime and stores its canonical runtime-native directory path, user-facing path, availability facts, non-removable flag, and revision.
- Root admission requires the selected directory to resolve inside the owning runtime's derived Host or WSL environment; cross-environment paths are rejected rather than silently creating or selecting another runtime.
- Runtime creation and creation of its non-removable user-home root are one revision-checked durable command. Neither record becomes visible alone.
- Each Syndic thread's immutable execution record stores the runtime id, root id, and canonical
  runtime-native root path needed for execution. It remains readable even if the Beryl configured-
  root record is later unavailable. Beryl joins that identity to registry and availability facts
  but does not own or revise the binding.
- Runtime and configured-root registries are additive. No runtime-removal, root-removal, or execution-rebind mutation is exposed.
- Runtime and root availability are observed facts, not destructive registry updates. Temporary absence does not erase configuration or thread bindings.
- Each runtime/root record owns a package-local monotonic record revision in addition to the enclosing domain revision. Availability updates require both expected revisions, retain a bounded availability category plus an optional caller-supplied Unix-millisecond observation time, and never use wall-clock time as mutation ordering authority.
- Root last-activity time is a presentation fact updated only by an exact later caller-supplied Unix-millisecond value. It does not become conversation or catalog authority.

## Cross-Domain Thread Relationships

- Beryl owns only application relationships to Syndic threads: configured runtime/root registry
  membership and availability observations, window selections and claims, durable orchestration-job
  references, and rebuildable catalog copies. A relationship record never becomes a second owner of
  title, execution, lineage, history, draft, archive, activity, or usage facts.
- Creating a thread atomically publishes its Syndic thread, immutable execution, attributes, usage,
  summary, and current draft with any Beryl runtime/root, session, and claim changes required by the
  admitting workflow. Beryl may validate the referenced runtime and root in that command without
  retaining a duplicate binding record.
- A successful branch-handoff command atomically advances the Beryl job and the Syndic thread's
  one-way automatic archive state. Neither side infers the other's success from a later projection.

## Thread Claims And Empty-Thread Acquisition

- A durable window id owns at most one active thread claim, and a Syndic thread has at most one active or restoring window claim.
- Each claim stores the exact session generation in which its current ownership/state was published and a separate monotonic claim revision. A later unrelated session publication does not rewrite an unchanged claim, but every claim-changing command validates both values as well as the current session revision.
- Claim-or-create is one serialized, revision-checked command over the exact runtime/root scope, catalog eligibility facts, thread record, current draft, and claim records.
- Eligible empty threads are ordered deterministically by oldest creation identity and then stable thread id. The first still-eligible unclaimed thread is reused; otherwise a new thread and empty current draft are created atomically.
- Claim eligibility is revalidated inside the writer. A stale catalog query revision or resident page cannot cause two windows to acquire the same thread.
- Thread claim release is coupled to the accepted replacement claim or durable window removal so a visible window is not left threadless once a runtime exists.
- Claims belonging to the durable restore set remain restoring claims across process restart. Claims not referenced by the active restore generation are stale and are released during validated startup before ordinary acquisition.
- The initial zero-runtime window has a durable window record without a thread claim. Adding the first runtime atomically creates the runtime, home root, thread/draft, window claim, and selected-thread/session updates.

## Session And Window Records

- The store maintains one active restore-set generation and one bounded window record per restorable main conversation window, with at most 256 restorable main windows in one Beryl home.
- The active header is one fixed-capacity record whose sorted unique window references carry each exact window id and expected window-record revision. Window records use a fixed-size V1 encoding, including canonical padding for optional identities and the bounded monitor hint, so bootstrap byte cost is determined solely by the 256-window limit.
- A window record stores durable window id, selected Syndic thread id when allowed, remembered runtime/root, position, size, monitor/work-area hints, Windows virtual-desktop identity, and its record revision.
- The active session header retains the runtime/root pair from the latest successful thread activation or empty-thread acquisition independently of window-record lifetime. This fallback survives ordinary closure of the final window.
- Auxiliary Settings windows, flyouts, menus, notices, and previews never receive session records.
- Window creation, successful thread activation, accepted geometry updates, and ordinary window close update the active generation through revision-checked commands.
- Ordinary healthy-home window close admits or joins the active turn's durable exact stop and
  removes the window record only after terminal-history convergence plus the dirty-draft and
  session flush barriers succeed. Only local pre-byte nondispatch while the target remains exact
  leaves the window open; a provider rejection without a current-target verdict converges through
  authority loss. Possible dispatch retains the close claim and waits for terminal or
  authority-loss convergence.
- Closing the final main window through ordinary close commits removal of the final window record, leaving the active restore set empty, and then terminates the process normally.
- Dedicated application Exit flushes the already-open window set and marks shutdown mode without processing those windows as ordinary closes.
- External process termination leaves the last `SyncAll`-completed active generation intact.
- In-memory window identity, placement, or selection retained during a failed store cannot survive process termination unless it was part of that last `SyncAll`-completed generation.
- During initial bootstrap, the `beryl` executable opens the home behind its startup fence,
  registers or structurally reacquires every required Beryl and Syndic domain into one private typed
  candidate, constructs every required dependent service, and atomically publishes the complete
  healthy stack.
- During running same-home recovery, the process-wide `beryl-app` home-recovery supervisor reopens
  that home behind its recovery publication fence, structurally reacquires the complete required
  domain stack into one private typed candidate, constructs the fresh dependent stack, and
  atomically publishes the complete newer healthy generation only after convergence succeeds.
  Routine reacquisition validates declarations and required physical families but is not an
  exhaustive application-record scan. Neither boundary may run session, restore-set, or other
  application discovery through partial registration, partial typed handles, or a session-only
  candidate.
- Only after complete-stack publication does the minimal discovery query read the active session
  generation and referenced fixed-size window records, reread the active header, and reject
  concurrent publication rather than combine generations. After that set is known and before each
  restored window becomes visible, startup performs one bounded typed read for that window's
  selected thread and current draft.
- If validated session discovery finds an empty restore set and at least one runtime, startup chooses the most recently used runtime and its most recently used still-configured root, falling back to that runtime's non-removable home root. One command then creates the replacement window record and claims or creates an eligible empty thread in that scope before the window becomes visible.
- If the restore set and runtime registry are both empty, startup creates the one permitted threadless initial window record.
- The pre-window path does not load the full catalog, transcript records, or CAS state.
- Invalid or duplicate records fail session validation. Beryl does not silently substitute another thread or window identity.

## Compact Thread Catalog

- Each Syndic thread has one compact catalog row projection keyed by stable thread id and source revision.
- A row contains only the resolved Syndic title and source, immutable runtime/root scope, automatic
  branch-discussion archive state, recent activity, availability, claim state, lineage summary,
  search normalization, exact source revisions, and deterministic ordering.
- Turn bodies, transcript items, Markdown, resource bytes, draft text, and CAS thread metadata are excluded.
- Source mutations update the row in the same commit when the required facts are available, or atomically mark that row stale for bounded background rebuild.
- Catalog authority maintains rebuildable title, runtime, root, availability, recency, and
  normalized-search indexes. Revision-bound recent, scope, and search queries return bounded
  deterministic cursor pages without constructing a complete in-memory metadata model.
- Equal-recency order uses stable thread id as the final tie-breaker.
- Search uses precomputed, schema-versioned Unicode `NFKC_Casefold` title, runtime-label,
  configured-executable-path, and full-root-path fields. Query text uses the identical fixed Unicode
  mapping and matches a contiguous normalized substring. It is lexical filtering only and is not
  the deferred semantic-search feature.
- An open flyout receives an immutable query revision and compact cursor authority. Background catalog updates publish a later revision for the next interaction instead of reordering the open collection.
- Query evaluation may perform work proportional to the durable catalog, but its local page,
  index-walk, and ordering state remain bounded by configured item and byte limits. GUI row
  construction remains fixed-height and virtualized.

## Crash And Fault Verification

- Release verification includes subprocess harnesses that terminate writers at every
  Beryl-controlled cut before commit, after commit but before `SyncAll`, and after `SyncAll`, then
  perform the routine structural reopen followed by a separately requested exhaustive scrub that
  validates every domain invariant. Release verification includes the owned Fjall fork's
  deterministic journal-write failure seam and proves that every such failure propagates through
  the classified command result without reported durable success.
- Theme-repository subprocess coverage terminates at staged-document creation and flush, staged
  owner-manifest creation and flush, manifest replacement, and parent-directory synchronization.
  Reopen proves one complete old or new repository generation, or retains exact indeterminate
  evidence for targeted reconciliation; an orphan staged file or mixed document/manifest set never
  becomes authoritative.
- Fault injection covers below-reserve turn-start admission, ordinary `ENOSPC`, denied writes,
  disappearing removable or network storage, truncated sidecar creation, failed rename, failed
  directory sync, process abort, panic, forced termination, repeated reopen failure, and bounded
  exact-codec-rejected physical record envelopes through the non-production `test-faults`
  boundary.
- Theme-file fault injection covers truncated staged documents and manifests, denied file writes,
  failed flush, failed manifest replacement, failed directory synchronization, and disappearance
  of the repository directory. Every case proves exact non-publication, exact durable publication,
  or indeterminate owner-manifest publication without reporting a mixed generation as committed.
- Open and lock tests cover concurrent processes, process death with the lock file left in place,
  canonical path aliases, initial `state` symlink, junction, and other reparse-point collision
  rejection, fully supported local NTFS homes, best-effort filesystem tiers, unreliable-lock
  rejection, and explicit orderly release.
- Concurrency tests cover competing draft saves, simultaneous different-thread submissions, claim-or-create races, session close versus Exit, resolution admission versus queued input, CAS binding transitions, and stale expected revisions.
- Admission tests cover all four free-space outcomes and prove exactly one immediate query for each
  direct or queued new turn, no query for steering, exact input preservation and zero CAS dispatch
  on denial, and ordinary later `ENOSPC` classification.
- Tests assert that no failed command is reported accepted, no two windows own one thread, no thread
  has two current drafts or active turns, and no durable metadata points at missing committed
  sidecar bytes. Recovery tests permit only the one exact correlated terminal CAS turn snapshot
  delegated for a repair-required turn and reject broader or repeated historical reads. They also
  prove that a fresh reopen remains private until one complete app-visible stack is published, and
  that neither initial bootstrap nor reopen discovery can observe partial domain registration or
  partial typed handles.
- Reconciliation tests independently fill all 1,024 registry slots and the 256-MiB aggregate byte
  budget, prove the next potentially indeterminate mutation receives typed pre-writer
  `NotCommitted` capacity evidence without global failure or interruption of already-admitted work,
  and reject descriptors above the 64-MiB per-descriptor ceiling.
  They also prove immediate reservation release for directly classified `NotCommitted` and
  `Committed`, one descriptor and no duplicate queue work per joined verifying scope, four-worker
  saturation, and exact-old and exact-new slot release. Collision tests prove opaque-descriptor
  disposal, retention of only one bounded sealed-fact set and its closed slot, and release of
  transient readers, snapshots, pages, hooks, and the worker permit.
- Provider-ingress fault tests force `Indeterminate` at every staging operation and prove the
  move-only reservation and descriptor reach the exact per-home registry before acknowledgement,
  cancellation, connection retirement, service disposal, or operation-state release. No path may
  publish, retry, fabricate a receipt, or drop custody; orderly final close cannot dispose a
  descriptor-bearing gate.
