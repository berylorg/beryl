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

Fjall 3.1.6 was subsequently proved source-identical at the affected path. The Operator reported the defect upstream as [fjall-rs/fjall#304](https://github.com/fjall-rs/fjall/issues/304) and, on 2026-07-13, explicitly approved using the exact official 3.1.6 release while awaiting the response.

Proceed through the ordinary Fjall batch-plus-`SyncAll` API and fail closed for every surfaced error. Keep the suppressed journal-write result as an explicit known dependency gap in `crates/beryl-home-store/doc/design.md`; do not mask it with an adapter, maximum-batch assumption, retry, dual write, or belief that write failures are permanently sticky. Do not claim that downstream fault verification proves this path safe.

Adopting a corrected upstream release or an owned fork remains a later explicit decision.

# Affected Authority

- `doc/plan.md` Phase 1 records the accepted exception and is complete.
- `doc/rework/beryl-home/REWORK.md` Checkpoint 2 records the resolved dependency gate and continuing known gap.
- `crates/beryl-home-store/doc/design.md` owns the package-level known issue.
- Any replacement Fjall artifact must still be re-investigated for batch error propagation, features, fail-closed forced recovery, snapshots, error variants, lock behavior, and fault-test support before adoption.
