# Reason For Investigation

Phase 41 requires `beryl-home-store` to admit every content-dependent point or cursor read
allocation before endpoint encoding or storage access. The investigation needed to determine
whether the exact resolved Fjall cursor can expose a bounded key or value-size lookahead before
materializing the value, whether iterator residency is derivable from caller and codec limits, and
whether a newer official release closes either gap.

# Outcome

The workspace resolves `fjall` 3.1.6 and `lsm-tree` 3.1.6 with Fjall's default `lz4` feature. The
home store uses an ordinary, non-key-value-separated keyspace. Its cursor calls `Snapshot::range`,
advances the returned iterator, consumes the guard through `Guard::key`, then separately calls
`size_of` and `get`.

For the ordinary tree, advancing the iterator has already produced
`Result<(UserKey, UserValue)>`. `Guard::key` merely discards the already-present value. Therefore an
oversized or malformed stored value can be materialized before the home store checks its codec or
caller byte limit, and the current home-store path may load the same value again through `get`.

Range construction also allocates a vector of boxed source iterators, adds one iterator for each
overlapping physical run and each sealed memtable plus the active and optional ephemeral memtable,
clones owned range endpoints for table and run readers, and constructs a merger over those sources.
That residency depends on the physical LSM topology rather than solely on the family key/value
maxima and caller item/stored-byte limits.

The latest official releases checked on 2026-07-21 are Fjall 3.1.8 and `lsm-tree` 3.1.8, both
published 2026-07-18. Their immutable tag sources preserve the same ordinary-tree guard and range
construction. Upgrading from 3.1.6 to 3.1.8 does not close either allocation boundary.

Fjall's official key-value-separation mode is a partial alternative for value payload loading. Its
blob-tree guard can return a key or size without resolving an indirect blob, and
`into_inner_if(false)` can omit that blob read. This does not bound cursor construction: blob-tree
range delegates to the same ordinary-tree internal range and merger. It is also a persisted
keyspace configuration rather than a compatible alternate cursor call, and its default 1 KiB
separation threshold leaves smaller values inline.

No released 3.1.8 public API found in the exact official sources supplies caller-owned fixed cursor
storage, a topology-independent iterator-residency bound, or a standard-tree key-and-value-size
lookahead before the standard `UserValue` exists. Consequently the Phase 41 cursor and exhaustive
physical-scan boundary cannot be proved from only the current family envelope and caller limits.
An upstream or targeted dependency boundary, a storage-layout/proof redesign, or an explicit
authority exception is required; this note does not select among them.

The point-read ordering is narrower: home-store calls `size_of` and validates the stored/caller
limit before `get`, so the full user value is not acquired through its public point path before
that check. Dependency-owned blocks and caches remain separate residency concerns.

## Effort Assessment

Further inspection for the implementation-effort question narrows what "value materialization"
means in the ordinary tree. A standard `UserValue` is a zero-copy slice into an already loaded and,
where configured, decompressed data block; constructing the guard does not necessarily make a
second payload copy. The allocation boundary is still too late because `Block::from_file` reads the
whole block and trusts its header's uncompressed length when allocating the LZ4 output before the
record guard exists. One oversized record can also force a data block above its target size.

The complete correction is not a small guard-method change, but a Beryl-specific bounded mode may
be a medium, localized dependency change rather than a replacement storage engine:

- A bounded block-load option could reject a compressed handle or declared uncompressed size before
  its buffer allocation and return a typed limit error. The existing block header, handle, parsed
  value offsets, and blob-tree guard provide most of the metadata needed for that seam.
- Fjall's ordinary mutation path already stalls while L0 has at least 30 runs and while at least
  four sealed memtables remain. Those hard-coded checks are not a complete public invariant:
  range construction does not preflight them, ingestion installs a run without calling the local
  backpressure path, and recovery accepts persisted topology. They nevertheless make an explicit,
  enforced maximum-source mode plausible.
- Such a mode would count overlapping sources before allocating the iterator vector or heap, reject
  topology above its configured ceiling, enforce the same ceiling across ordinary writes, batches,
  ingestion, and recovery, and let callers admit the fixed worst case. The existing merger could
  then remain proportional to the configured ceiling rather than the unbounded observed topology.
- If Fjall must instead support arbitrary run counts with workspace independent of any enforced
  ceiling, the merge algorithm needs a substantial redesign or repeated rescanning with a serious
  CPU/I/O tradeoff. That is not an easy patch.

The resulting estimate is therefore split: pre-allocation block limits and a hard bounded-source
read mode look like focused cross-cutting work with persistence, corruption, concurrency, and
performance tests; a genuinely constant-workspace generic LSM iterator is a larger algorithmic
project. Key-value separation can assist the first design by keeping large payloads out of merge
heads, but still needs bounded blob loading and an intentional persisted-layout cutover.

# Sources

- crates.io package `fjall` 3.1.6, checksum
  `9fcdc69609906151dff9b534e30eaf8515082055d36f628e382bd0b5d6a1d362`, resolved by
  `Cargo.lock`; the root manifest enables the default `lz4` feature. Inspected `src/guard.rs`,
  `src/readable.rs`, and `src/snapshot.rs` from the local registry on 2026-07-21. Package:
  https://crates.io/crates/fjall/3.1.6
- crates.io package `lsm-tree` 3.1.6, checksum
  `39ca67401338b98d58447387dd5230552d2241bc388206e491d137b18dfea9d6`, resolved transitively
  by Fjall with `lz4`. Inspected `src/tree/mod.rs`, `src/blob_tree/mod.rs`, `src/range.rs`,
  `src/slice/slice_default/mod.rs`, and `src/config/mod.rs` from the local registry on 2026-07-21.
  Package: https://crates.io/crates/lsm-tree/3.1.6
- Official Fjall repository tag `3.1.8`, commit
  `6debe706dbc53d6d0eb666aae5057671d5c1370f`. Inspected `src/guard.rs`, `src/readable.rs`, and
  `src/keyspace/options.rs`, `src/keyspace/mod.rs`, and `src/ingestion.rs` on 2026-07-21:
  https://github.com/fjall-rs/fjall/tree/6debe706dbc53d6d0eb666aae5057671d5c1370f
- Official `lsm-tree` repository tag `3.1.8`, commit
  `f09f4235c5e6735c54f99c0d425784602ce71975`. Inspected `src/tree/mod.rs`,
  `src/blob_tree/mod.rs`, `src/range.rs`, `src/merge.rs`, `src/table/iter.rs`,
  `src/table/data_block/mod.rs`, `src/table/block/mod.rs`, `src/table/util.rs`, and
  `src/config/mod.rs` on 2026-07-21:
  https://github.com/fjall-rs/lsm-tree/tree/f09f4235c5e6735c54f99c0d425784602ce71975
- Official `lsm-tree` iterator API RFC #110, opened 2025-02-27 and completed for 3.0.0. It records
  that key and size access avoids payload loading specifically for key-value-separated blobs:
  https://github.com/fjall-rs/lsm-tree/issues/110
- Official `lsm-tree` issue #257, opened 2026-03-14 and closed as not planned, documents the same
  pre-validation allocation class for blob decompression lengths:
  https://github.com/fjall-rs/lsm-tree/issues/257
- Official release metadata for Fjall 3.1.8 and `lsm-tree` 3.1.8, including the 2026-07-18
  publication date and exact dependency pairing, accessed 2026-07-21:
  https://docs.rs/crate/fjall/3.1.8 and https://docs.rs/crate/lsm-tree/3.1.8
- Local use sites: `Cargo.toml`, `Cargo.lock`,
  `crates/beryl-home-store/src/store/opening.rs`,
  `crates/beryl-home-store/src/read/execute.rs`, and
  `crates/beryl-home-store/src/domain/registered.rs`.

# Commands

- `git ls-remote --tags https://github.com/fjall-rs/fjall.git '*3.1.8*'`
- `git ls-remote --tags https://github.com/fjall-rs/lsm-tree.git '*3.1.8*'`
- Targeted `rg` searches and line inspection over the resolved registry sources and local use sites.
