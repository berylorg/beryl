# Scope

Checkpoint 2 of the Beryl-home storage rework, specifically the use of official crates.io Fjall 3.1.5 and 3.1.6 for crash-durable cross-domain commits.

# Invalidated Approach

Use `Database::batch` / `OwnedWriteBatch::commit` for one atomic cross-keyspace mutation, then call `Database::persist(PersistMode::SyncAll)` before reporting durable success.

The normal success path and crash recovery support that shape, so the dependency initially appeared to satisfy the target contract.

# Evidence

Exact Fjall 3.1.5 source at upstream commit `41bc2136e5979289ba92a32797afae72fe693ab8` contains this sequence in `src/batch/mod.rs`:

- Line 117 discards `journal_writer.write_batch(...)` with `let _ = ...`.
- Lines 119–128 propagate only the subsequent persistence failure.
- Later code applies the complete batch to memtables and publishes its visibility.

`journal_writer.write_batch` uses fallible `Write::write_all` calls for the batch start, items, and end checksum. If one of those calls returns a transient or intermediate error, the unwritten remainder is not recreated merely because a later buffer flush and `SyncAll` succeed. Both Fjall transaction modes ultimately delegate to the same batch commit path.

The full dependency investigation is preserved in `doc/memory/crates.io/fjall/3.1.5/home-store-atomicity-and-durability.md`.

# Why It Failed

Beryl could receive success from `commit` and the later `SyncAll` even though the journal lacks a complete recoverable batch. The full batch would remain visible in the running process but could disappear on reopen because recovery discards incomplete journal batches.

That violates the authoritative rule that Beryl must never report a failed storage mutation as durably saved. It also makes the required full-disk, interrupted-write, and transient-I/O proofs impossible for exact 3.1.5.

# Course Correction

Fjall 3.1.6 was subsequently proved source-identical at the affected path. The Operator reported the
defect upstream as [fjall-rs/fjall#304](https://github.com/fjall-rs/fjall/issues/304). The interim
decision to use that release while preserving the known gap is now superseded by Beryl's owned
Fjall fork.

The owned fork propagates every journal-write failure before in-memory batch publication, requires
an explicit durability mode when constructing a checked-capacity batch, and exposes stable commit
state for every failure. Beryl's accepted sequence is a `PersistMode::Buffer` batch followed by an
explicit `Database::persist(PersistMode::SyncAll)` barrier before reporting durable success.
Deterministic non-production faults cover the formerly suppressed journal-write path.

Beryl must cut production callers directly to that final API. No adapter, retry, dual write, or
continued use of the inherited `OwnedWriteBatch` surface is an accepted correction.

# Affected Authority

- `../fjall-fork/doc/design.md` owns the corrected batch and durability contract.
- `../fjall-fork/doc/rework/bounded-storage-api/REWORK.md` tracks final fork acceptance.
- `doc/plan.md` Phase 42 accepts that fork boundary before the later Beryl caller cutover.
- `crates/beryl-home-store/doc/design.md` owns Beryl's batch, persistence, health, and fault-test
  requirements.
