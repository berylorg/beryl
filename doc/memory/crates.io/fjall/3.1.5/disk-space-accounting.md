# Reason For Investigation

Operator asked why the Syndic storage benchmark reported `fjall` disk usage as 67,108,864 bytes for both small and large profiles, and whether fjall documentation explains the large apparent size or how users should predict file growth.

# Outcome

This note records the original crates.io 3.1.5 benchmark observation. The live workspace now uses
Beryl's owned `fjall` fork at package version 3.1.6 with package-owned storage policy and journal
footprint APIs. The preallocation explanation remains historical background, not current admission
or configured-limit authority.

The 67,108,864-byte value is explained by fjall 3.1.5's active journal preallocation.

Local resolved source for `fjall 3.1.5` defines `PRE_ALLOCATED_BYTES` as `64 * 1_024 * 1_024` in `src/journal/writer.rs`, then calls `file.set_len(PRE_ALLOCATED_BYTES)` when creating and rotating journal files. `Database::disk_space()` includes the active journal length plus sealed journal bytes and keyspace disk space, so a fresh or small database can report at least one 64 MiB journal even before table data dominates.

For Syndic's current benchmark, the `fjall` adapter uses one keyspace with `KeyspaceCreateOptions::default` and reports `database.disk_space()`. That means the observed constant 67,108,864 bytes should be read as fjall's database-level allocated/accounted footprint, not as payload bytes. It is not evidence that fjall serialized the small benchmark payload into 64 MiB of table data.

To predict fjall growth, model at least these components:

- Active journal allocation: currently one 64 MiB preallocated journal is included in `Database::disk_space()`.
- Additional sealed journal files: controlled by write volume, flushing, and journal maintenance; `DatabaseBuilder::max_journaling_size` defaults to 512 MiB and cannot be configured below 64 MiB.
- Keyspace table files: each keyspace is its own physical LSM tree; table growth depends on rows, values, compression, filters, indexes, block size, and compaction.
- Memtable/flush behavior: default keyspace `max_memtable_size` is 64 MiB, and flushing/compaction is deferred/background.
- Optional key-value separation: default blob file target size is 64 MiB when enabled, but the current Syndic benchmark adapter does not enable key-value separation.

Future benchmark passes should report journal disk space separately from keyspace/table disk space if fjall remains under consideration, and should consider a "steady after flush/compaction/reopen" measurement distinct from immediate post-write `Database::disk_space()`.

For the scale benchmark diagnostics, fjall 3.1.5 exposes enough counters through public or doc-hidden public methods to avoid guessing journal state from filenames. `Database::journal_count()`, `Database::journal_disk_space()`, `Database::write_buffer_size()`, `Database::outstanding_flushes()`, `Database::active_compactions()`, `Database::compactions_completed()`, and `Database::keyspace_count()` are callable from the local adapter. `Keyspace::disk_space()`, `Keyspace::sealed_memtable_count()`, `Keyspace::l0_table_count()`, `Keyspace::table_count()`, and `Keyspace::blob_file_count()` expose keyspace-level counters for the single benchmark keyspace.

# Sources

- Local `Cargo.lock`, inspected 2026-06-16. Resolved `fjall = 3.1.5` and `lsm-tree = 3.1.5`.
- Local source `C:/Users/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fjall-3.1.5/src/journal/writer.rs`, inspected 2026-06-16. Relevant lines: `PRE_ALLOCATED_BYTES` at line 19; `file.set_len(PRE_ALLOCATED_BYTES)` at lines 131 and 164.
- Local source `C:/Users/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fjall-3.1.5/src/db.rs`, inspected 2026-06-16. Relevant lines: `write_buffer_size` at line 209; `outstanding_flushes` at line 220; `active_compactions` at line 247; `compactions_completed` at line 260; `journal_count` at line 280; `journal_disk_space` at line 290; `disk_space` combines journal and keyspace sizes at lines 314-326; `keyspace_count` at line 493.
- Local source `C:/Users/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fjall-3.1.5/src/db_config.rs`, inspected 2026-06-16. Relevant lines: default `max_journaling_size_in_bytes` 512 MiB at line 77.
- Local source `C:/Users/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fjall-3.1.5/src/keyspace/mod.rs`, inspected 2026-06-16. Relevant lines: `sealed_memtable_count` at line 279; `disk_space` at line 394; `l0_table_count` at line 844; `table_count` at line 851; `blob_file_count` at line 858.
- Local source `C:/Users/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fjall-3.1.5/src/keyspace/options.rs`, inspected 2026-06-16. Relevant lines: default `max_memtable_size` 64 MiB at line 91; `with_kv_separation` at line 519; block-size space-efficiency note at lines 670-675.
- `fjall` crate documentation, version 3.1.5, docs.rs, accessed 2026-06-16. Useful for LSM-tree design, per-keyspace physical LSM trees, `Database::disk_space`, `DatabaseBuilder::max_journaling_size`, `KeyspaceCreateOptions`, `KvSeparationOptions`, and durability behavior.
