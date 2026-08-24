# Goals

Own the typed durable schemas and APIs for Beryl-owned application state inside one Beryl home.

Provide revision-checked runtime/root, session/window, settings, catalog, durable-job, and asset-
metadata domains over the shared physical home store without exposing storage-engine details or
absorbing Syndic thread ownership.

## Non-goals

- Opening or locking the Beryl home, owning Fjall, serializing the physical writer, or choosing persistence barriers.
- Owning Syndic threads, execution bindings, titles, archive state, usage observations, drafts,
  turns, items, projections, resources, compact thread summaries, or CAS-binding records.
- Owning product workflows, GPUI presentation, backend launch, CAS protocol, transcript rendering, or feature-specific GUI behavior.
- Importing workspace-era records, supporting compatibility schemas, or providing dual reads and writes.
- Owning physical installed-theme file placement, handles, or durability, or heavy asset and Syndic
  sidecar bytes.

# Decisions

## Public Boundary

- `beryl-state` registers the Beryl-owned logical record families required by `doc/systems/beryl-home-storage/design.md` through the typed-domain boundary exposed by `beryl-home-store`.
- It owns each domain's exact Rust owner type, one exact codec type per family, record schemas,
  exhaustive domain validation, operation-scoped natural-record reconciliation hooks, typed bounded
  queries, mutation validation, revision rules, and batch contributions. Stable names and schema
  versions remain durable declarations rather than substitutes for live type identity.
- Every owned codec enforces its stored and decoded schema limits through the bounded home-store read
  boundary. Ordinary point and fixed bootstrap values carry no accounting wrapper; their limits
  pass before publication. Natural pages and composite paged reads report checked item,
  stored-byte, and decoded-byte totals where useful. Callers retain only a configured number or
  coarse byte budget of pages rather than accumulating a complete domain.
- It receives an opaque registered-domain handle. It never receives or exposes a raw database, keyspace, Fjall batch, transaction, writer guard, lock, or byte encoding.
- Callers use stable Beryl and Syndic identity values plus expected revisions. Stored key layout and codec versions remain private.
- Commands that affect only Beryl domains execute through this package's typed command contributors. Commands that must be atomic with Syndic changes contribute both domain mutations to one `beryl-home-store` command coordinated by the owning system.
- The package exposes a checked maximum mutation-footprint descriptor for its asset-owner-transfer
  participant, selected by the typed draft-to-submitted-item or accepted-input-to-submitted-item
  transfer operation. The descriptor derives its maximum record count and encoded key-plus-value
  bytes from the package-owned Asset V2 head shapes, including the marker-free validation-only
  case, with checked arithmetic and without opening a home.
- The descriptor covers only this package's Asset-domain participant. It accepts no caller byte
  estimate and excludes Syndic mutations, home-store participant metadata and home revision, Fjall
  journal framing, capture reserve, filesystem allocation, and admission policy.
- State commands preserve `NotCommitted { evidence }`,
  `Committed { receipt, later_failure }`, and `Indeterminate { failure, reconciliation }` without
  erasing, wrapping, or reclassifying their proof. `NotCommitted` proves no commit and carries no
  receipt or reconciliation custody. `Committed` always carries the exact
  process-local home generation, committed home revision, and affected Beryl-domain revisions even
  when its optional later failure is present. Only `Indeterminate` carries the opaque
  operation-scoped descriptor together with its complete already-reserved registry capacity, carries no receipt,
  and authorizes no publication. The package immediately moves that sole custody value onward
  unchanged to the command coordinator under the synchronous per-home registry-handoff contract in
  the [Beryl-home storage system](../../../doc/systems/beryl-home-storage/design.md); it retains no
  copy, domain-local ambiguity flag, or separately publishable result. Each Beryl domain projects
  only its own receipt-bound revision through its opaque state handle, returns `None` when
  unaffected, and never exposes the underlying home-store domain handle.
- Initial open and same-home recovery construct the complete `BerylState` handle set against one
  unpublished healthy home generation. This package neither constructs the surrounding app/backend
  stack nor owns its publication fence; its handles become caller-visible only as part of the
  composition owner's complete typed-stack publication. Prior-generation handles, prepared
  commands, sidecar tokens, and receipts cannot authorize candidate or later work; an old receipt is
  rejected even when its revisions equal current records.

## Runtime And Root Domain

- Runtime records store stable runtime id, canonical absolute Codex CLI executable identity, derived exact Host or WSL-distribution mode, exact runtime-native executable path, user-facing environment label, creation facts, observed availability summary, and revision.
- Root records store stable root id, owning runtime id, canonical runtime-native path, user-facing full path, non-removable fact, availability summary, activity facts, and revision.
- Root admission rejects a directory whose canonical filesystem environment does not match the owning runtime's derived Host or WSL mode.
- One canonical Codex executable identity has at most one runtime record. Multiple executable runtimes may belong to the same Host or WSL environment.
- Canonically equivalent roots under one runtime cannot produce duplicate root records.
- Runtime creation plus its non-removable user-home root is one validated batch contribution; neither record may publish alone.
- Runtime and root records are additive; this package exposes no removal command contributor.
- Runtime and root ids, canonical paths, environment mode, availability observations, creation times, and activity times are already admitted caller facts. This package generates none of them and performs no filesystem, clock, WSL, process, or CAS observation.
- Every runtime and root record has a package-local nonzero monotonic record revision. Mutation contributors require its exact expected value as well as the enclosing home and domain revisions.
- Catalog joins read one exact runtime/root pair and carry that pair into a validation-only
  participant. The serialized writer snapshot must still contain the same records and root-id
  index before another domain may publish their projection.
- Availability stores the bounded shared availability category plus an optional caller-supplied Unix-millisecond observation time. Unknown availability has no observation time; wall-clock order is never conflict authority.
- Root activity is only an optional caller-supplied Unix-millisecond presentation maximum. An update must be strictly later than the stored value.

## Thread Relationship Boundary

- This package owns no canonical per-thread metadata record. A Syndic thread's immutable execution
  binding, title, archive state, activity, usage, lineage, draft, and history remain in
  `syndic-storage`.
- Beryl-owned records may reference a stable Syndic thread id only for an application relationship:
  runtime/root availability joins, window selection and claims, durable host jobs, asset-owner
  heads, and rebuildable catalog projection rows. None of those records may duplicate intrinsic
  thread authority or expose a mutation for it.
- Cross-domain commands validate and mutate final Beryl and Syndic records atomically through
  `beryl-home-store`; this package owns no duplicate relationship record and does not absorb either
  domain's final authority.

## Session And Window Domain

- One `beryl-session` domain owns the active header, window records, claims by window, and claims by
  thread. Keeping those families together lets its exhaustive domain validator prove exact session
  and reverse-claim invariants through the sealed per-domain reader when an exhaustive validation
  boundary is requested.
- The active session header stores generation, orderly-Exit intent, sorted unique restore-set window references, and the last successfully used runtime/root fallback retained when the restore set becomes empty. Each reference contains the exact window id and expected window-record revision.
- One home supports at most 256 restorable main-window records. The V1 header uses a fixed-capacity encoding and each V1 window record uses a fixed-size encoding with canonical tagged padding for optional identities and monitor facts; all-zero identities remain ordinary valid identities rather than absence sentinels.
- Each restorable main-window record stores window id, selected Syndic thread id, remembered runtime/root ids, placement and size facts, virtual-desktop identity when supported, and record revision.
- Auxiliary windows, flyouts, menus, notices, and previews never receive session records.
- Session commands preserve exclusive thread claims and reject missing or multiply referenced window/thread identities.
- The only valid threadless record is the sole zero-runtime initial window; it has no remembered target, selected thread, claim, or fallback.
- Startup privately registers or reacquires every required Beryl domain into the complete candidate
  without an exhaustive application-record scan, then publishes that handle set only with the full
  stack.
  The later minimal discovery query still returns only the fixed-size session header and exact
  referenced window records, rereads the header, and rejects concurrent publication. It loads no
  catalog, transcript, CAS, or placeholder-draft state.

## Thread Claims

- Claim records key exact `WindowId` and Syndic thread id with the publishing `SessionRevision`, active-or-restoring state, and a separate monotonic `ClaimRevision`.
- A window owns at most one active or restoring claim, and a thread has at most one active or restoring window owner.
- Claim, release, restore, and claim-or-create contributions validate both reverse indexes in the serialized home-store command.
- Stale asynchronous observations cannot clear or replace a newer claim generation.
- Paired claims outside the active header/window set are retained only as bounded stale startup state and are removed by the revision-checked begin-restore command. Missing or disagreeing reverse copies remain corruption.
- Claim records are durable coordination facts for crash/session recovery; they are not OS locks and do not replace the one-process home lock.
- Catalog joins use one bounded thread-claim point read that proves the thread and window reverse
  copies agree, or proves the thread-keyed copy absent. The resulting present-or-absent fact is an
  exact validation-only participant in the cross-domain publication command.

## Theme Domain Service

- This package owns the typed repository, schema, parser, validator, resolver, mutation, and
  reconciliation service required by the [theme runtime system](../../../doc/systems/theme-runtime/design.md).
  It consumes physical repository and durability operations from the Beryl-home storage boundary
  without exposing raw files or storage handles to callers.
- The service owns stable installed theme ids, repository generations, manifest cursor pages,
  document revisions and digests, the finite role/property schema, bounded compact-TOML parsing,
  and complete resolved appearance values.
- Manifest generations own only installed membership, bounded names, and order. An observed
  manifest identity additionally binds the exact physical byte length and digest, so an external
  same-generation rewrite cannot reuse a cursor or command identity. Each stable
  `installed/<stable-theme-id>.toml` document receives a process-local observation revision plus
  exact length and digest whenever the physical repository reports a change or a command publishes
  replacement bytes; returning to an earlier digest still creates a newer observation revision.
- Install, rename, delete, reorder, update, Save, and Save As use revision-checked typed commands
  with exact `NotCommitted`, `Committed`, or `Indeterminate` results and targeted
  `ExactOld`, `ExactNew`, or `Collision` reconciliation.
- Before physical admission, the service reserves one slot in a fixed-capacity retained-operation
  registry shared by all clones of that exact service. An indeterminate outcome moves its evidence
  and expected publication into that registry; the public result exposes only an operation id, so
  dropping the caller handle cannot lose custody or reopen the affected scope. The registry permits
  only one reconciliation flight for an operation. `ExactOld` and `ExactNew` remove the retained
  scope, while `Collision` retains it as permanently closed for the service lifetime.
- Repository-scoped custody gates all repository mutation and repository refresh. Document-scoped
  custody gates overlapping mutation and reread of that exact stable id without blocking unrelated
  manifest paging. A separately acquired service lifecycle owns a distinct bounded registry; the
  composition-owned `BerylState` retains its service and returns shared clones rather than silently
  reacquiring it. The registry has no process-global map or self-retaining owner and is released
  when the last service clone and activity guard retire.
- Save As validates the exact original draft binding while canonicalizing the published copy with
  its newly allocated stable id, leaving the original draft unchanged.
- Delete execution reacquires an authoritative bounded Settings and open-draft reference snapshot;
  a caller-carried reference claim is only an expected fact and cannot authorize deletion by
  itself.
- Public reads and queries are point, range, or revision-bound page operations with explicit item
  and decoded-byte limits. No API returns the complete installed collection or requires a whole
  document to be resident.
- A document load reobserves the exact manifest membership row and the exact document identity
  after streaming and immediately before returning a publishable typed document. A manifest or
  document race therefore returns a typed retry/freshness failure instead of publishing a value
  assembled across physical observations.
- The service consumes bounded coalesced physical-repository change hints, maps only manifest-
  admitted stable filenames, rereads through the physical range API, and applies the same parser,
  validator, and resolver used by commands. Invalid live edits retain the last coherent typed
  appearance input; invalid startup content yields the built-in fallback input. It never watches
  paths directly or treats a notification as bytes, revision, or commit proof.
- This package owns no GPUI entities, window subscriptions, preview arbitration, feature editor
  draft, Settings draft, tool worker, or visible failure presentation.
- `ThemeService::diagnostics` returns content-free bounded facts only: generation presence, active
  manifest sessions, subscriptions and document loads, watcher coalescing/overflow counts, mutation
  and reconciliation outcome counts, retained open/collision scope counts, and load retry
  rejections. Activity guards decrement their counts on ordinary drop or explicit shutdown.

## Settings Domain

- Settings records store only validated Beryl-owned scalar preferences and stable setting-schema versions.
- Feature-owned setting schemas define accepted values and defaults. The
  [settings feature](../../../doc/features/settings/design.md) owns their product workflow; this
  package supplies typed scalar storage and revision behavior without redefining those semantics.
- Installed theme documents and order remain in the separate theme repository service above. Only
  active theme identity and declared scalar theme settings use the Settings domain.
- Backend-owned Codex configuration, authentication, skills, MCP, sandbox, policy, and session state are rejected from Beryl settings records.
- Schema V1 is a closed typed set: active theme identity, context-compaction timeout, draft-autosave interval, developer instructions, and optional end-turn-sound Host path. There is no arbitrary key or byte-payload variant.
- Active-theme identity is bounded to 256 UTF-8 bytes and developer instructions to 60 KiB. Numeric preference values are caller-validated feature scalars; this package preserves their exact typed representation without taking ownership of feature ranges or defaults.
- Context-compaction timeout is stored as an exact caller-validated millisecond scalar under its
  record revision. Its accepted range, absent-value default, and operation meaning are owned by the
  [status-line feature](../../../doc/features/status-line/design.md); this package supplies no
  semantic default and does not interpret expiry.
- One settings mutation is nonempty and duplicate-free, validates every absent-or-exact record
  expectation before mutation assembly, and advances every affected record atomically or none of
  them.
- Each key carries its exact supported setting-schema version. Unknown setting schemas and
  unsupported record versions fail any read that decodes them and every applicable exhaustive
  validation boundary rather than being interpreted as defaults; routine structural reopen alone
  does not scan dormant settings.

## Durable Job Domain

- Durable job records own Beryl orchestration that must survive restart, including branch-resolution handoff admission, ordered attempts, exact target identities, idempotency keys, lifecycle state, bounded failure evidence, and job revision.
- Job schemas are typed by job kind. A generic payload blob cannot bypass owning-system validation.
- Job transition commands require the exact prior state and revision and cannot regress a terminal success or reuse an attempt identity.
- Job records store no authentication material, capability tokens, hidden developer instructions, or unbounded model/tool payloads.
- Schema V1 owns only branch-discussion handoff jobs. Each record retains exact intent, ordered attempt, child, parent, context owner and digest, resolving Syndic turn, correlated CAS tool request, parent queue ordinal, admitted resolution, parent Syndic/CAS identities when reached, lifecycle state, and job revision.
- Job ids are deterministically derived from the admitted resolution-intent identity. Exact CAS thread, turn, and tool-call identity forms the durable request-idempotency key; repeated delivery is answered from that index rather than creating another job.
- Lifecycle states are exactly `waiting_resolving_turn`, `waiting_parent`, `starting_parent`, `parent_active`, `retryable_failed`, `terminal_failed`, and `succeeded`. Retryable failure retains the exact checkpoint and resumes the same job and parent identities; terminal states leave the live-job index and never regress.
- Runtime unavailable, root unavailable, CAS unavailable, and delivery failure proven before dispatch are retryable at every non-failure checkpoint. Exact CAS rejection before acceptance is retryable only at `starting_parent`. Remote completion unknown after possible parent-start dispatch is not retryable; proven execution-session loss converges the parent turn incomplete and the job terminally. Invariant violation and missing parent are terminal at every non-failure checkpoint; unrecoverable post-append is terminal only at `starting_parent` or `parent_active`; parent interruption, incomplete termination, and terminal failure are terminal only at `parent_active`.
- One compatibility rule owns both retryable-versus-terminal classification and checkpoint
  eligibility. Mutation admission and every persisted-record decode use that same rule. A bounded
  recovery read rejects an incompatible record it actually decodes, and exhaustive validation
  rejects every incompatible stored job record; routine structural recovery does not claim to scan
  dormant jobs.
- Resolution text is bounded to 64 KiB and failure detail to 2 KiB. Failure kinds must agree with retryability and with the retained lifecycle checkpoint.
- Records, live jobs, request admissions, ordered discussion attempts, and latest-attempt pointers
  occupy separate typed families whose exhaustive domain validator proves their two-way agreement.

## Catalog Domain

- Catalog records are compact rebuildable projections keyed by Syndic thread id and deterministic recency indexes.
- Rows contain only the resolved Syndic title/source, runtime/root scope, automatic branch-
  discussion archive state, recent activity and completeness, claim/availability state, lineage summary, bounded
  title/environment/executable-path/root-path search normalization, exact source revisions, and
  catalog revision.
- Turn bodies, transcript items, draft content, Markdown, resource bytes, and CAS metadata are excluded.
- Rebuild inputs are bounded Syndic thread-catalog summaries plus authoritative Beryl runtime/root
  availability and session-claim facts. A catalog row never becomes the source of either domain's
  record.
- Complete cursor reads expose every matching compact row under explicit byte and row accounting while GUI row realization remains separately virtualized.
- Stale or invalid catalog projections are marked and rebuilt; they are never accepted as durable proof for correctness-sensitive mutation.
- Schema V1 stores one authoritative projection row keyed by Syndic thread id and one full compact row copy in a recent-first index. Recency keys invert the activity timestamp and use ascending thread id as the deterministic equal-time tie-break.
- Row facts preserve the Syndic-resolved title and source, runtime and root ids and full paths,
  environment label, availability, completeness, claim, branch archive, bounded lineage, exact source revisions,
  projection freshness, and a package-local catalog revision. The row does not retain separate title
  candidates or recompute their precedence.
- Compact lineage mirrors the bounded Syndic summary: a top-level marker, or exact parent id,
  nonzero `u64` depth, and selected-path digest. It does not invent a top-level id or retain the
  complete ancestry query.
- The catalog domain owns one `NFKC_Casefold` implementation shared by publication and query input.
  Normalization profile V1 is Unicode R5 `toNFKC_Casefold` over fixed Unicode 17.0.0 `NFKC_CF`
  data followed by NFC from exactly pinned Unicode 17 tables. Schema V1 stores the profile identity
  and rejects any decoded search key that does not recompute from its original visible facts.
  Entirely removed text produces an ordinary empty key. The package validates the normalized text
  and byte ceilings: 2 KiB for title, 1 KiB for environment label, and 64 KiB each for executable
  and root path. Query construction rejects either original or normalized text above 64 KiB before
  it can become catalog cursor authority.
- One row or recency record is bounded to a 256 KiB payload. Point limits pass before a bare row
  publishes; natural cursor pages report practical stored and decoded totals. Reads fail rather
  than silently returning an apparently complete collection outside caller bounds.
- Publishing a changed recency fact replaces the old index key in the same contribution. Marking
  stale updates the row and index copy together; exhaustive domain validation rejects missing,
  duplicate, or disagreeing copies.

## Asset Metadata Domain

- Asset records store opaque asset id, versioned digest and byte length, validated media facts, sidecar state, creation revision, and asset revision.
- Asset domain schema V2 represents marker ownership through immutable paged reference sets. One
  sealed set manifest stores one content-neutral `SequentialMarkerSummaryV1`, entry frontier, and
  asset-chain digest;
  bounded ordered entries map stable marker identities and labels to exact asset ids and carry
  validated first-occurrence disposition. Build-local durable label-first keys derive that
  disposition without an in-memory seen-label set. Compact owner-head records bind one current
  draft, accepted input, submitted item, retry record, or
  transcript projection to one sealed set and owner revision.
- Asset schema V2 is the only supported asset record shape. This package exposes no V1 decoder,
  migration path, dual write, or compatibility adapter.
- Reference-set construction is typed unpublished staging. Bounded commands append entries and a
  final seal proves the exact `SequentialMarkerSummaryV1`, entry frontier, and asset-chain digest. One final
  cross-domain home command swaps only compact source/destination owner heads while publishing the
  matching Syndic admission; it never moves one Fjall record per marker in that transaction.
- Nonempty current-draft replacement-set construction accepts bounded ordered marker entries and the target
  `SequentialMarkerSummaryV1`, validates every staged entry and exact EOF/frontier/count/
  maximum/digest agreement, and returns only the existing opaque sealed-set proof. The package does
  not accept or authenticate a Syndic draft root, build identity, marker-tree commitment, or marker-
  seal proof, and it has no dependency on `syndic-storage`; a public summary alone authorizes no
  owner-head mutation.
- For current-draft publication, this package contributes exactly one Asset participant. A changed
  nonempty marker set validates the opaque sealed-set proof and swaps the exact CurrentDraft head; a
  changed empty set accepts the empty-removal branch only after Syndic has validated its completed
  seal and exact empty sequential summary, then validates the exact prior head/revision transition
  and removes that head, requiring no Asset proof or synthetic empty set; an
  unchanged nonempty set asserts the existing head and proof; and an unchanged marker-free set
  asserts absence. Syndic, not this package, decides unchanged versus changed from its authenticated
  draft commitments. No Asset outcome independently reports whole-command success.
- Marker-free admission carries one bounded, duplicate-free typed validation-only owner-head
  participant in the same home command as the mutating Syndic participant. It checks the exact
  optional state of every source and destination head on the serialized writer snapshot and
  asset-domain revision fence, emits no mutation, advances no asset revision, and is absent from the
  receipt's affected domains. Owner-head mutation and validation use one exact-state rule; an
  owner-head mutation participant may include bounded no-write assertions for unchanged heads
  alongside at least one actual put, replacement, or removal. An all-assertion batch is `NoEffect`
  and cannot disguise validation-only work as an empty contribution. This lets reference-set copy
  assert its historical head and publish its new owner through one Asset-domain participant;
  duplicate same-domain participants remain invalid.
- Sidecar bytes and runtime-path projection remain outside this package. `AssetState` owns
  construction of the private `images` content address and streams exact-length/digest verification
  through bounded pages. The verification boundary returns bounded file-backed authority without
  reading or mapping the complete asset into memory.
- Removing an owner head does not delete bytes or require proving that it was the final reference.
- `AssetId` is the shared SHA-256-v1 digest plus exact nonzero `u64` byte length. Asset schema V2 has
  no smaller whole-asset byte ceiling and retains an ASCII graphic media type of at most 127 bytes,
  optional nonzero dimensions, creation domain revision, committed-sidecar state, and metadata
  revision. Representable-length exhaustion is explicit; it is not a process-memory limit.
- Durable owner variants are current draft, accepted input, submitted-turn item, retry record when a
  separate retry snapshot exists, and transcript projection. Each head selects an immutable marker-
  reference set. Queued, steering, delivering, retryable, and delivery-unknown states reuse the
  accepted-input owner; they are not separate reference identities. Transient clipboard tokens are
  not written as durable references.
- Durable next-turn promotion atomically removes the exact accepted-input owner head and publishes
  the exact fresh submitted-turn-item head over the same sealed set. Marker-free promotion asserts
  both heads absent, and neither form reads or mutates the current-draft owner head.
- First metadata publication requires the matching admitted `images` sidecar authority. Digest,
  length, namespace, and media facts must agree before metadata publication; a crash may leave
  unreferenced metadata or a sealed reference set, but no owner head can select missing bytes.
- Package-private repair-asset staging records are keyed by the existing Syndic target thread,
  turn, item, resource ordinal, and asset identity; they introduce no shared repair-snapshot
  identity. This package's participant in the system-owned bounded repair-media stage command
  consumes the current-generation `AdmittedSidecar` and records exact digest/length, media facts,
  authenticated repair provenance, and the command's matching media commitment. It publishes no
  ordinary asset metadata, owner head, transcript reference, or canonical resource disposition.
  Ordinary asset APIs cannot read or select these records.
- The sole repair publication participant validates the complete expected staging set and every
  final sidecar through bounded verification, then promotes the exact asset metadata, references,
  and resource dispositions. It participates only in the final cross-domain `HomeCommand` defined
  by `doc/systems/cas-live-syndic-transcript/design.md` and cannot independently assert whole-command
  success. Missing, disagreeing, or unstaged Beryl-state media rejects this participant. A failed or
  incomplete repair leaves its staging records and prepared sidecars inert and unreachable; only a
  future home-wide garbage collector may reclaim them.
- A nonempty mutable draft reference change streams every ordered marker page supplied during the
  captured Syndic seal into one replacement set, then atomically swaps the draft owner head. A
  changed-to-empty reference instead removes the exact head through the no-proof branch above.
  Duplicate markers, label/asset disagreement, stale owner revision, count overflow, sequential-
  summary mismatch, and digest mismatch reject publication.
- Owner-head removal retains asset metadata and the sidecar. There is no metadata deletion or
  byte-deletion command and no eager zero-reference proof.
- Public entry reads clamp caller limits to the package's fixed page-item and stored-byte ceilings;
  label-first lookups are bounded point reads used after a Syndic origin-span lookup.
- Sealed manifest, entry-page, and label-first reads require the complete opaque
  `SealedAssetReferenceSetProof`, not a caller-supplied set id. Every read rechecks the selected
  manifest against that proof's exact sequential marker, frontier, and digest authority before
  returning data; a different proof for the same set identity is rejected. Unsealed construction
  inspection uses separate typed build authority and cannot select sealed state. A bare set id
  never authorizes a sealed-manifest read.
- Exhaustive asset-domain validation walks owner heads and sealed set entries in bounded pages,
  proves every selected asset metadata record and sidecar, and checks exact set
  counts/digests/frontiers. It does not build a reverse reference set or trust a mutable metadata
  reference count; future garbage collection must perform its own bounded reachability pass.

## Bounds And Validation

- Every non-fixed read requires explicit row, byte, or range limits. Exact catalog streaming additionally reports accumulated byte cost and stops with a typed bound failure if the documented compact-domain invariant is violated.
- Read accessors return ordinary bounded typed values or natural pages. Point limits are enforced
  internally without attaching a charge to the returned value; pages report applicable item,
  stored-byte, and decoded-byte totals. A configured read-limit failure remains direct typed
  home-store read provenance and is not semantic corruption.
- Stored labels, paths, normalized search fields, failure evidence, setting values, and job payloads have schema-specific byte limits enforced before batch contribution.
- Asset-owner-transfer footprint tests enumerate the maximum encoded Asset V2 owner-head keys and
  values for both transfer operations, cover the marker-free validation-only shape, and checked-sum
  their record and byte totals. An unrepresentable result is a package contract failure; the
  package never saturates, wraps, or accepts a caller-provided replacement estimate.
- Ordinary mutation validation and contribution perform only operation-bounded typed reads. They never invoke a complete domain scan while holding the home writer.
- Routine open, same-home reopen, and typed-handle reacquisition validate the exact owner and codec
  types, durable domain and family declarations, required physical families, and generation. They
  do not visit every application record or run complete domain invariants.
- Only a schema-validation boundary, explicit or background whole-home scrub, or investigation
  triggered by separate corruption evidence first visits every raw record envelope through the
  family's exact codec and stored-key legality hook. Beryl-state's exhaustive validators then check
  runtime/root uniqueness, non-removable roots, session/window references, claim reverse
  uniqueness, job state machines, catalog projection and exact-source-revision agreement, and
  asset-metadata-to-sidecar references with bounded memory.
- Each mutation domain also supplies a separate bounded reconciliation hook over only the command's
  natural records. It classifies its exact descriptor-bound observations as `ExactOld`, `ExactNew`
  with the receipt facts needed for reconstruction, or `Collision`. A mixed old/new observation or
  one matching neither exact side is `Collision`; the hook never scans the domain, guesses or
  merges state, requests a whole-home scrub, clears writer poison, or treats CAS as authority.
- Repair-media staging reconciliation observes only the target turn/item/resource natural identity,
  inert prepared-asset record, and exact sidecar commitment. Final repair publication
  reconciliation observes that evidence plus the canonical asset metadata, reference, and resource
  disposition named by the same command. It contributes those sealed old/new facts to the
  cross-domain classifier; it never turns a Beryl-only successor into repair success or exposes
  inert staging through an ordinary asset read.
- Reconciliation collision keeps only the command's operation scope closed. It is not structural
  corruption unless another admitted read or validation boundary supplies separate structural
  evidence; unrelated structurally healthy domain work remains admissible.
- Every mutation and validation error explicitly extracts a direct typed home-store read failure, and asset validation additionally extracts a direct sidecar failure. Other errors remain semantic domain rejections; Beryl-state does not hide storage provenance behind an arbitrary source chain.
- Exhaustive validation reports authoritative corruption instead of dropping records, narrowing a
  scan to typed cursor sentinels, or consulting CAS history.

## Fault-Test Boundary

- The `test-faults` Cargo feature exposes only typed schema-corruption contributions needed to prove persisted Beryl-state rejection. Normal builds contain no corruption command, alternate codec, compatibility reader, or storage bypass.
- Durable-job corruption fixtures preserve otherwise coherent indexes so tests prove the exact failure-kind/checkpoint rule rather than succeeding because an unrelated index invariant failed first.
- Domain tests prove exact-old, exact-new receipt-fact, mixed, and neither-state reconciliation hook
  classifications without giving a hook a whole-domain cursor or raw storage authority.

## Dependency Boundary

- This package may depend on `beryl-model`, `beryl-home-store`, serialization/validation support, Unicode normalization, and cryptographic digest primitives required by its owned schemas.
- It must not depend on `gpui`, `beryl-app`, `beryl-backend`, CAS protocol types, or `syndic-storage` private record types.
- Cross-domain coordination uses stable public Syndic identities and typed `beryl-home-store` participants rather than one package reaching into another package's records.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `persistent-state-integrity/v1`
