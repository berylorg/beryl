# Reason For Investigation

Checkpoint 2 Phase 1 needs proof that the exact declared Fjall 3.1.5 target can implement one Beryl-home physical store with atomic cross-domain mutations, an explicit `SyncAll` durability barrier, bounded concurrent reads, fail-closed reopen, private keyspace registration, and permanent fault/crash testing without a production adapter that obscures Fjall behavior.

The Operator authorized the locally cached crates.io package source as the exact Phase 1 proof target even though no live workspace package consumes Fjall yet. The permanent `beryl-home-store` consumer and its project `Cargo.lock` entry are Phase 2 work.

# Outcome

This note is historical evidence for the original crates.io 3.1.5 evaluation. The live workspace
now resolves Beryl's owned `fjall` fork at package version 3.1.6, whose design, failure records, and
tests own the corrected journal, recovery, configured-limit, and Windows durability boundaries.
Do not use this note as current exact-fork evidence or infer that its 3.1.5 blocker remains open.

## Proof Target And Verdict

The inspected artifact is crates.io `fjall` 3.1.5, archive SHA-256 `038acd422d607e0eca09e093f299f9eccf9bd097554343d93746afff81a45113`, published from upstream commit `41bc2136e5979289ba92a32797afae72fe693ab8`. The target is the Windows Beryl-home store described by the controlling system and package designs.

The root workspace declaration is currently `fjall = "3.1.5"`, with no explicit feature override. There is no `fjall` package in the project `Cargo.lock` yet. For this exact artifact, the prospective default feature set is therefore `lz4`; `bytes_1`, `metrics`, and `__internal_whitebox` remain disabled unless Phase 2 changes the declaration. The manifest gives `lsm-tree` the requirement `~3.1.5` with its defaults disabled and forwards `lz4`. Phase 2 must still verify the real Fjall version, transitive versions, target, and enabled features after the permanent consumer exists.

The normal-path atomicity, snapshot, persistence, and reopen primitives exist, but exact Fjall 3.1.5 has a concrete error-propagation blocker for the accepted fault contract: `WriteBatch::commit` discards the result of the journal batch write. The target therefore cannot prove that every failed journal commit is rejected under I/O faults. Raw Fjall handles or iterators also cannot escape `beryl-home-store`.

## Cross-Keyspace Batch Atomicity

- `Database::batch` creates a `WriteBatch`; the public re-export is `OwnedWriteBatch`. `WriteBatch::{insert, remove}` accept a `&Keyspace`, and `WriteBatch::commit` writes all items with one batch sequence number.
- The journal representation is one `Entry::Start { item_count, seqno }`, the items containing their keyspace ids, and one checksum-bearing `Entry::End`. `JournalBatchReader` yields a batch only after the declared count and checksum match. An incomplete final batch is truncated back to the last valid position and discarded.
- `WriteBatch::commit` applies every item at the common sequence number, then calls `SnapshotTracker::publish`. A `Database::snapshot` opened during application retains the earlier visible sequence and sees none of the batch; a snapshot opened after publication can see all of it. Recovery likewise replays complete journal batches and publishes visibility only after recovery.
- Cross-keyspace readers must therefore use `Database::snapshot` plus the `Readable` methods. Direct `Keyspace` reads are not a sufficient atomic-view boundary: several use `SeqNo::MAX`, and direct `Keyspace::range` and `prefix` do so even though they retain a snapshot nonce.
- Fjall does not check that a keyspace passed to `WriteBatch::insert` belongs to the batch's database. It journals the supplied handle's numeric id and mutates that handle's tree. `beryl-home-store` must create every registered handle from its one database, keep handles private, and never accept a raw caller-supplied keyspace.
- `WriteBatch` has no intermediary reads and is not a read-modify-write transaction. The accepted Beryl design remains valid because its one writer validates typed expected revisions immediately before assembling the batch.

## Commit And Persistence Barrier

- With the default `DatabaseBuilder::manual_journal_persist(false)`, `Database::batch` configures the batch with `PersistMode::Buffer`. On an error-free path, `commit` flushes Fjall's buffered journal bytes to operating-system buffers before applying and publishing the memtable changes, but it does not promise power-loss or OS-crash durability.
- A corrected engine target preserving this API can implement the design's explicit two stages by retaining that default, calling `OwnedWriteBatch::commit`, then calling `Database::persist(PersistMode::SyncAll)` while still on the serialized writer. Both operations take the same journal mutex, so the barrier orders after the committed journal bytes; writer serialization prevents unrelated writes from being included accidentally.
- `PersistMode::SyncAll` first flushes Fjall's `BufWriter` if dirty and then calls `File::sync_all` on the active journal. It does not create or publish a batch, flush memtables into tables, sync Beryl sidecars, or provide a directory durability barrier.
- `OwnedWriteBatch::durability(Some(PersistMode::SyncAll))` can fuse the file barrier into `commit`, but that erases the call-site distinction needed by the accepted commit-versus-barrier health handling and crash cut points.
- If `commit` succeeds and the following `Database::persist(SyncAll)` fails, the batch is already visible in memory and its eventual recovered presence is indeterminate. The operation must not report durable success; the store must enter its verification/failure gate and recover the same home rather than attempt an in-memory rollback.
- A persist failure sets Fjall's database-wide poison flag and returns `Error::Poisoned`; the underlying I/O error is logged but not preserved in the returned variant. Later writes and persistence calls reject with `Poisoned`. Beryl can still classify the failure as a persistence-barrier failure from the operation stage.
- In 3.1.5, `WriteBatch::commit` explicitly discards the direct result of `journal_writer.write_batch` before performing its configured persist. All single-writer and optimistic transaction commit paths delegate to this same batch commit.
- The Rust `Write::write_all` contract returns the first non-interrupted write error before guaranteeing the entire input was written, while `flush` covers contents retained by the buffering writer. The default `Buffer` persist catches a persistent buffered flush failure and poisons the store, but it cannot propagate Fjall's already discarded `write_all` error or recreate an unwritten remainder. If a later flush succeeds after such an error, Fjall proceeds to apply and publish the complete memtable batch even though the journal may be incomplete or corrupt. A following `SyncAll` can then succeed.
- Consequently, exact 3.1.5 cannot establish the design rule that no failed command is reported accepted across commit I/O faults. `manual_journal_persist(true)`, fused `SyncAll`, or the transactional database wrappers do not repair the discarded result. Resolving this requires an upstream Fjall version or owned fork that propagates `write_batch(...) ?`, or explicit Operator acceptance of a weaker assumption such as an engine-internal write error never becoming transient. The accepted plan forbids silently taking that weaker guarantee, so Phase 1 is blocked on this decision.

## Open, Drop, Close, And Reopen

- `DatabaseBuilder::open` is create-or-recover. `Database::create_or_recover` selects recovery only when the database `version` marker exists; otherwise it calls `create_new`. A same-home recovery path that must never initialize an empty replacement cannot call this generic path blindly after failure.
- Exact 3.1.5 exposes `Database::recover(Config)` and `DatabaseBuilder::into_config`, so a forced, fail-closed reopen is technically possible. Both symbols are `#[doc(hidden)]`; the exact version must remain locked, their use must stay internal, and Phase 2 needs a focused compile/reopen test. Missing or invalid version state then fails instead of selecting `create_new`.
- Recovery checks format version V3, acquires Fjall's internal lock, recovers the active journal, performs `SyncAll` on it, reacquires stored keyspaces, replays complete batches, and starts workers. Journal or structural errors remain reopen failures.
- Fjall has no fallible `close` or `shutdown` API. Orderly close must stop admission, wait for all in-flight typed reads/writes, explicitly call `Database::persist(SyncAll)` so errors are observable, and then drop every database, keyspace, batch, snapshot, guard, and iterator owned by the generation.
- `Journal::drop` makes a best-effort `SyncAll`, but only logs an error. It cannot substitute for the explicit barrier. `DatabaseInner::drop` stops workers and clears cyclic registries without returning a result.
- Every `KeyspaceInner` clones Fjall's `LockedFileGuard`; an uncommitted batch owns a `Database` and cloned keyspaces. Dropping only the main `Database` can therefore leave the internal database lock held and cause reopen to return `Error::Locked`. No read cursor or raw handle may escape the generation being drained.
- The internal lock file is reusable: acquisition opens an existing file and uses non-blocking `File::try_lock`; `WouldBlock` maps to `Error::Locked`. Recovery retries that condition three times with 100 ms waits. Unlock failure during guard drop is only logged.
- Fjall's internal `fsync_directory` is a no-op under `cfg(target_os = "windows")`. `SyncAll` proves an active-journal file barrier, not Windows directory durability for Fjall metadata or Beryl sidecars.

## Keyspace Creation And Registration

- `Database::keyspace(name, create_options)` is create-or-open. If the name is already registered, Fjall returns the recovered handle and does not evaluate or compare the supplied options. If absent, it creates the physical tree and records the id, name, and encoded options in Fjall's internal meta keyspace.
- `Database::{keyspace_exists, list_keyspace_names}` expose the recovered registry. There is no separate public get-only keyspace call. Reopen validation must inspect the existing name set before calling `keyspace`, so a missing required authoritative keyspace fails validation instead of being silently recreated.
- Keyspace creation is not an item in the user `WriteBatch` and cannot be made atomic with domain records through that batch. Register the complete stable family during fresh-home/opening state, persist and validate it, and only then publish a healthy generation.
- Invalid keyspace names panic in `Database::keyspace`; the implementation currently checks non-empty UTF-8 names whose byte length fits `u8`, despite broader wording in its comment. Domain names must be static, prevalidated, and at most 255 bytes.
- `Database::delete_keyspace` exists and physical deletion is deferred until handles drop. The home-store boundary should not expose it for required authoritative domains.

## Bounded Point, Range, And Iterator Reads

- `Database::snapshot` provides one repeatable cross-keyspace view. `Readable::{get, contains_key, size_of, range, prefix, iter}` operate on that view, and `Snapshot::{range, prefix, iter}` retain a snapshot nonce for the iterator lifetime.
- Point reads are naturally item-bounded. `Readable::size_of` allows a typed point API to reject an oversized value before fetching it.
- Range and prefix APIs accept lexical `RangeBounds` and return a lazy, double-ended `Iter`. Standard iterator `take(max_items + 1)` can enforce an item limit and detect truncation. The typed wrapper must also require a finite key range or non-empty prefix and drain the iterator inside the read command.
- Fjall has no built-in byte budget, deadline, or result-size error for scans. `Iter` yields `Guard`, and key/value access can itself return an error. Typed codecs must enforce per-record size limits at admission; the wrapper must accumulate decoded bytes and propagate every `Guard::into_inner`/access error without returning raw guards or iterators.
- Whole-keyspace `iter` and `len` are explicitly documented as potentially expensive. They are unsuitable for feature reads. Validation or catalog loading may stream them only behind an explicit item/byte bound derived from the registered domain contract.

## Error Surfaces

Fjall's public `Error` is `#[non_exhaustive]`. Exact 3.1.5 variants are `Storage(lsm_tree::Error)`, `Io(std::io::Error)`, `JournalRecovery`, `InvalidVersion`, `Decompress`, `InvalidTrailer`, `InvalidTag`, `Poisoned`, `KeyspaceDeleted`, `Locked`, and `Unrecoverable`.

The cached `lsm-tree` 3.1.5 nested error is also non-exhaustive and contains `Io`, `Decompress`, `InvalidVersion`, `Unrecoverable`, `ChecksumMismatch`, `InvalidTag`, `InvalidTrailer`, `InvalidHeader`, and `Utf8`. `JournalRecoveryError` contains `InsufficientLength`, `TooManyItems`, `ChecksumMismatch`, and `InvalidFileName`.

The home-store mapping should retain the operation stage and source chain, then classify at least:

- `Locked` as an internal-database busy/undrained-handle condition, distinct from the outer Beryl-home ownership result.
- Direct or nested I/O during open/read/commit as an engine I/O failure.
- Invalid versions, checksum/tag/trailer/decompression failures, journal recovery errors, and unrecoverable state as structural/open-validation failures.
- `Poisoned` from the explicit post-commit `SyncAll` call as a persistence failure with indeterminate committed state.
- `KeyspaceDeleted` or a missing required registry name as an invariant/registration failure.
- Any future non-exhaustive variant as an opaque engine failure that closes the health gate rather than being guessed safe.

Batch item construction panics on an empty key or a key longer than 65,535 bytes. The typed codecs must enforce much smaller bounds before batch assembly. Error classification cannot rely only on Fjall's display string; it prints debug-oriented variants, and the upstream README advises applications to treat storage failures as restart/recovery events.

## Fault Injection And Permanent Testability

Fjall 3.1.5 has no built-in failpoint, injectable filesystem/journal, fault feature, or downstream test utility. Its internal `#[cfg(test)]` modules are unavailable to consumers. The optional `__internal_whitebox` feature only exposes global drop counters through `fjall::drop`; it does not inject commit, sync, recovery, or I/O failures and should remain disabled.

The permanent strategy can preserve direct production use of Fjall:

- Keep production code as explicit calls to the concrete `OwnedWriteBatch::commit`, `Database::persist(SyncAll)`, sidecar file operations, and forced `Database::recover`; do not introduce a production storage trait or alternate engine behavior.
- Compile Beryl-owned cut-point callbacks only for tests around the concrete boundaries: before commit, immediately after commit/before `SyncAll`, immediately after `SyncAll`, before/after each sidecar write-flush-rename-directory-barrier step, and before/after forced reopen. These callbacks may abort a subprocess or return a stage-specific synthetic failure to test health gating; they are absent from production builds.
- Run subprocess crash fixtures against the real Fjall database at before-commit, post-commit/pre-`SyncAll`, and post-`SyncAll` acknowledgements. Force termination, reopen the same home, and validate that every cross-keyspace invariant is wholly old or wholly new as permitted by that cut, never partially applied.
- Exercise real reopen errors with disposable corrupted/missing-version fixtures, denied paths, and deliberately retained handles. Exercise actual commit or file-sync I/O errors with platform-provided full/denied/disappearing storage fixtures when available; Fjall itself cannot make those deterministic.
- Keep a focused regression that a two-keyspace batch survives explicit `SyncAll`, complete handle drain, forced recovery, and bounded snapshot reads. Keep another proving that a retained keyspace prevents reopen, so the generation drain cannot regress.

The test-only callback strategy can prove Beryl's stage-specific state machine and crash cuts without weakening production semantics. It cannot repair or prove around Fjall's discarded batch-write result, and it does not by itself prove an operating system's exact failure behavior. Real I/O-fault fixtures remain required if a corrected engine target is selected.

## Constraints If A Corrected Engine Target Is Selected

- Use only keyspaces registered from the one owned `Database`.
- Validate revisions, collapse duplicate logical mutations, and assemble one batch on the serialized writer.
- Commit with automatic `Buffer` behavior, verify that the selected engine propagates the journal batch-write result, then perform the explicit `SyncAll` barrier before durable success.
- Serve every typed read from a short-lived `Database::snapshot` and enforce range, item, and byte bounds in the wrapper.
- On any structural or persistence uncertainty, close admission, drain all generation-owned objects, explicitly persist when still possible, and force-recover the same database.
- Refresh this note against any proposed Fjall version or fork, specifically proving propagation of the journal write result, then recheck features, forced recovery, snapshots, error variants, and failpoint support.

# Sources

Inspected 2026-07-13.

## Workspace Authority And Resolution State

- Root `Cargo.toml`: workspace dependency declaration `fjall = "3.1.5"`, with no feature or default-feature override.
- Root `Cargo.lock`: no `fjall` package entry at inspection time, as expected before the permanent Phase 2 consumer.
- `doc/plan.md`, Checkpoint 2 Phase 1: Operator resolution authorizing cached Fjall 3.1.5 as the proof target and requiring the real lock/features check in Phase 2.
- `doc/systems/beryl-home-storage/design.md`: cross-domain batch, `SyncAll`, bounded-read, fail-closed reopen, health, and crash/fault contracts.
- `crates/beryl-home-store/doc/design.md`: package ownership, typed registration, durability, and recovery boundary.
- Sibling memory `doc/memory/crates.io/fjall/3.1.5/disk-space-accounting.md`: existing exact-version source identity and journal background.

## Exact Registry Artifact

- crates.io package `fjall` 3.1.5: <https://crates.io/crates/fjall/3.1.5>.
- Cached crate archive SHA-256: `038acd422d607e0eca09e093f299f9eccf9bd097554343d93746afff81a45113`.
- Package `.cargo_vcs_info.json`: upstream repository <https://github.com/fjall-rs/fjall>, full source commit [`41bc2136e5979289ba92a32797afae72fe693ab8`](https://github.com/fjall-rs/fjall/commit/41bc2136e5979289ba92a32797afae72fe693ab8).
- Registry-relative source root: `$CARGO_HOME/registry/src/<crates.io-source-id>/fjall-3.1.5`.
- `Cargo.toml.orig` lines 1-35: package/version/Rust version, features, `lsm-tree ~3.1.5`, and dependency feature forwarding.
- `README.md` lines 59-180 and `src/lib.rs` lines 1-74: public usage, durability distinction, cross-keyspace claim, snapshot/transaction caution, drop behavior, and feature documentation.

## Atomicity And Durability Source

- `src/db.rs` lines 146-198: `Database::snapshot`, `Database::batch`, and default `PersistMode::Buffer` selection; lines 329-370: `Database::persist`; lines 384-416: create-or-recover dispatch.
- `src/batch/mod.rs` lines 12-181: `WriteBatch`, cross-keyspace item APIs, durability selection, journal serialization, poison handling, memtable application, and snapshot publication. The discarded `write_batch` result is line 117.
- `src/tx/write_tx.rs` lines 311-350, `src/tx/single_writer/write_tx.rs` lines 361-368, and `src/tx/optimistic/write_tx.rs` lines 434-454: both transaction modes route commit through `OwnedWriteBatch::commit` and inherit the same discarded journal-write result.
- `src/batch/item.rs` lines 8-72: each item owns a `Keyspace`, encoding bounds, and panic validation.
- `src/journal/entry.rs` lines 13-84 and 116-243: start/item/end encoding, keyspace id, count, checksum trailer, and decode errors.
- `src/journal/writer.rs` lines 32-50 and 202-232: `PersistMode` definitions and the `BufWriter::flush` plus `File::{sync_data, sync_all}` implementation; lines 327-375: complete-batch journal writing.
- `src/journal/batch_reader.rs` lines 18-73 and 76-215: complete-batch assembly, checksum/count checks, and incomplete-tail truncation.
- `src/journal/mod.rs` lines 38-53 and 97-116: best-effort `SyncAll` during journal drop, writer mutex, and journal persistence.
- `src/journal/test.rs` lines 57-116 and 226-487: upstream unit coverage for multiple-keyspace journal entries and corrupt/incomplete-tail truncation.
- `src/db_test.rs` lines 212-243: upstream two-keyspace batch recovery exercise.

## Lifecycle, Registration, Reads, And Errors Source

- `src/db.rs` lines 66-115: `DatabaseInner::drop`; lines 418-530: delete/create-or-open/list/exists keyspace APIs; lines 540-805: forced recovery, lock acquisition, complete-batch replay, visibility publication, and worker restart; lines 808-915: fresh database creation.
- `src/builder.rs` lines 23-63: `Builder::{into_config, open, manual_journal_persist}`.
- `src/locked_file.rs` lines 11-81: reference-counted internal lock guard, `try_lock`, error mapping, retry behavior, and drop unlock.
- `src/keyspace/mod.rs` lines 60-158 and 299-359: keyspace ownership and cloned lock guard; lines 400-473 and 577-698: direct iter/range/prefix/point reads; lines 900-1020: direct write ordering.
- `src/keyspace/name.rs` lines 5-14: actual name validation.
- `src/meta_keyspace.rs` lines 22-125 and 176-200: persistent name/id/options registry creation and lookup.
- `src/snapshot.rs` lines 9-104, `src/snapshot_tracker.rs` lines 72-127, and `src/readable.rs` lines 9-300: cross-keyspace snapshot reads, visibility publication, and range/scan cautions.
- `src/iter.rs` lines 7-40 and `src/guard.rs` lines 7-62: lazy iterator lifetime and fallible guard access.
- `src/error.rs` lines 9-83 and `src/journal/error.rs` lines 27-52: Fjall and journal recovery error enums.
- Cached `lsm-tree` 3.1.5 `src/error.rs` lines 7-58: nested storage error variants. Fjall's packaged `Cargo.lock` records `lsm-tree` 3.1.5 with checksum `8ef86c3c797c10eefcc73407c43ae48c19d4df686131a8334b2895a513e91df4`; the Beryl project must verify its own transitive lock in Phase 2.
- `src/file.rs` lines 16-34: directory sync implementation and Windows no-op.
- `src/drop.rs` lines 5-25 plus all `__internal_whitebox` references: drop counters only; no fault injector.

## Rust Standard Library I/O Contract

- Rust project, `std::io::Write` documentation, <https://doc.rust-lang.org/stable/std/io/trait.Write.html>, accessed 2026-07-13. `write_all` loops until all input is written or returns the first non-interrupted error; `flush` pushes intermediately buffered contents to the destination. This establishes why discarding the earlier `write_all` result is not repaired merely by observing a later flush result.

## Verification Commands

- `rg --files doc/memory/crates.io/fjall` to enumerate sibling memory before investigation.
- `rg -n -A 15 -B 2 'name = "fjall"' Cargo.lock` to prove the project lock has no current Fjall package.
- `Get-FileHash -Algorithm SHA256 .../fjall-3.1.5.crate` to identify the cached crates.io artifact.
- `rg -n '__internal_whitebox|failpoint|fault|inject|injection|test_utils|cfg\(test\)' src Cargo.toml.orig README.md` to audit test/fault facilities.
- Numbered `Get-Content` inspection of every source range listed above, plus `rg` symbol searches for batch, persist, open/recover, lock/drop, keyspace, read, and error APIs.
- `rg -n -A 100 -B 20 'fn commit|pub fn commit|write_batch' src/tx/...` to verify that neither transactional mode supplies an independent corrected commit path.
- No disposable compilation was needed: the exact implementations and packaged upstream unit tests directly expose the required behavior. No dependency was installed.
