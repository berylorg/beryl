# Beryl Home Phase 41 Fjall Range Residency

## Scope

Point, cursor, and exhaustive durable reads through `beryl-home-store` and the owned Fjall and
`lsm-tree` forks.

## Invalidated Approach

Require Fjall to publish an exact compound database and per-operation residency envelope so Beryl
could reserve dependency bytes and structural allocation slots before configuration, path access,
or reads.

## Evidence

- The inherited Fjall range boundary materialized values before Beryl could inspect stored length,
  and its cursor workspace grew with admitted LSM sources. The retained investigation remains in
  [`cursor-read-allocation-boundary.md`](../memory/crates.io/fjall/3.1.6/cursor-read-allocation-boundary.md).
- Metadata-first selection and explicit merge-source/topology limits are practical dependency
  improvements and remain required.
- Extending that fix into a complete quote required public formulas for shared cache, memtables,
  retained versions, pinned blocks, reader multiplicity, background work, path capacities, and
  allocator-dependent structural slots.
- `lsm-tree::Config` also normalized a `PathBuf` before optional quote validation, while ordinary
  `PathBuf` capacity is not an honest exact portable byte contract. Satisfying the quote literally
  would have forced a broad path-ownership redesign unrelated to observed product behavior.
- The Operator replaced exact process-wide accounting with risk-based bounds for the major
  CAS/Syndic/application/renderer dataflow and other concrete amplification points.

## Why It Failed

The full residency envelope coupled Beryl to dependency-private implementation details and claimed
precision that allocator and external-process behavior cannot provide. It was substantially more
complex than the storage risk warranted.

## Course Correction

- Keep metadata-first point and cursor reads so stored length is visible before separated-value
  acquisition.
- Keep generous encoded/decoded block and value limits, merge-source and retained-topology sanity
  limits, configured cache and memtable budgets, bounded read pages, and batch record/encoded-byte
  limits.
- Remove `Residency`, structural-slot, path-capacity, complete database-baseline, and incremental
  operation-quote requirements from Beryl, Fjall, and `lsm-tree` authority.
- Let `beryl-home-store` enforce its own item, stored-byte, decoded-byte, page, batch, and
  concurrency limits without reconstructing dependency allocation formulas.

## Affected Authority

- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/beryl-home-storage/design.md`
- `crates/beryl-home-store/doc/design.md`
- `doc/rework/beryl-home/REWORK.md`
- `doc/plan.md`
- the owned Fjall and `lsm-tree` design and plan documents

## Remaining Risks

- Large inline values must still be rejected or represented under a practical stored-value ceiling;
  metadata-first blob acquisition alone is insufficient.
- Cursor source and topology ceilings must remain enforced so malformed storage cannot request
  unbounded reader collections.
- Storage stress tests should watch process high-water behavior, but exact allocator equality is no
  longer an acceptance condition.
