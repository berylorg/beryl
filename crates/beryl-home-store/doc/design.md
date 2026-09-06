# Goals

Own the reusable physical storage and process-ownership boundary for one Beryl home.

Provide typed, revision-checked, bounded, crash-durable coordination across registered Syndic and
Beryl metadata domains without exposing Fjall internals to application or backend packages.

## Non-goals

- Owning conversation, draft, window, runtime, root, settings, catalog, job, asset, theme, GUI,
  backend, or CAS product semantics.
- Importing workspace-era state or providing compatibility reads, dual writes, migration adapters,
  raw-storage escape hatches, or a universal process resource governor.
- Detecting or surviving same-user external replacement, rollback, or tampering inside an
  Operator-selected trusted home. Surfaced or validation-visible disagreement still fails closed.

# Decisions

## Package Boundary

- The package owns one Fjall database, one serialized writer-admission permit, the home lifecycle
  and health gate, the process-local reconciliation registry, typed domain registration, bounded
  typed reads, atomic commands, and the physical theme and sidecar repositories for one home.
- No public API exposes a raw Fjall database, keyspace, batch, snapshot, cursor, guard, encoding,
  writer guard, filesystem handle, or arbitrary repository path. Domain packages exchange only
  their typed keys, values, participants, readers, results, and opaque capabilities.
- The package consumes typed domain definitions from `syndic-storage` and Beryl metadata packages
  without making either depend on the other's private records. It does not depend on `gpui`,
  `beryl-app`, `beryl-backend`, or CAS protocol types.
- The [Beryl-home storage system](../../../doc/systems/beryl-home-storage/design.md) owns
  cross-package storage interpretation and orchestration. The
  [bounded-resource system](../../../doc/systems/bounded-resource-dataflow/design.md) owns the
  shared rules for large reads, pages, staging, queues, caches, and scans. The
  [theme-runtime system](../../../doc/systems/theme-runtime/design.md) owns interpretation above
  this package's physical repository boundary.

## Documentation Set

This entry point owns the package boundary and engineering-rigor contract. The following documents
are normative supplements governed by this design and only for their stated roles:

- [Domain API](design-domain-api.md) owns domain registration, live type and attachment identity,
  bounded typed reads and results, and read-only cross-domain proof composition.
- [Physical open and recovery](design-open-and-recovery.md) owns home layout and open inputs,
  unpublished candidates, structural lifecycle and health, same-home recovery, and whole-home
  scrub boundaries.
- [Atomic commands](design-atomic-commands.md) owns command, result, receipt, durability,
  preparation, reconciliation, free-space query, local-finalization, and operation-scope contracts.
- [Physical repositories](design-physical-repositories.md) owns physical installed-theme and
  sidecar file layout, publication ordering, evidence, and watcher guarantees.

No supplement owns cross-package behavior outside its role or declares a separate rigor profile.

## Public And Dependency Boundary

- `HomeOpenOptions` contains only one absolute home path and one supported home-schema version; it
  never accepts or exposes a Fjall policy type. The opener applies one package-owned validated
  practical Fjall and storage-concurrency profile, and every recovery reconstructs that profile
  from retained package values. Open and recovery yield private candidates; only the owning system
  boundary may publish a complete healthy typed stack.
- Public failures preserve distinct busy, unsupported-schema, lock-unsupported, open, validation,
  conflict, persistence, sidecar, resource-limit, and health-gate provenance. Public command and
  repository results never turn ambiguous persistence into success.
- The package depends on the unpublishable [owned Fjall fork](../../../../fjall-fork/doc/design.md)
  and its independently versioned [owned LSM-tree fork](../../../../fjall-fork/crates/lsm-tree/doc/design.md),
  plus platform file-lock and filesystem primitives. It requires immutable mandatory block,
  separated-value, topology, cache, memtable, journal, and batch limits; metadata-first stored-value
  inspection; one coherent cross-keyspace snapshot; atomic batch publication; stable commit-state,
  error-class, health, and recovery reporting; and an explicit `SyncAll` barrier. Every database
  generation uses a fresh configuration and dependency cache.
- The `test-faults` feature may expose deterministic, bounded seams only at concrete package calls
  for reads, commit and persistence, verification and reopen, sidecar and theme-file operations,
  exact typed mutation scopes, exact-codec-rejected persisted records, and retained maintenance
  terminals. Production builds expose none of those seams and no alternate engine, virtual
  filesystem, raw storage access, compatibility path, or retry behavior.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `persistent-state-integrity/v2`

This declaration governs this entry point and every normative supplement in the Documentation Set.
The supported operating envelope is an Operator-selected trusted Beryl home with the documented
local-NTFS and best-effort filesystem tiers; hostile or same-user out-of-band replacement,
rollback, and tampering are outside it. Within that envelope, an affected write, durability,
recovery, or publication failure must not corrupt previously valid published state or falsely
acknowledge a new durable state.

Review findings that lose exact outcome classification, receipt or generation identity, format or
type fencing, enforced bounds, indeterminate-operation custody, publication ordering, or recoverable
prior state are blocking. The contract relies on Fjall's coherent snapshots, atomic batches,
surfaced commit state and failures, and `SyncAll` together with package-owned targeted
reconciliation; it does not require a global post-read health canary or exhaustive validation of
unrelated persisted state.
