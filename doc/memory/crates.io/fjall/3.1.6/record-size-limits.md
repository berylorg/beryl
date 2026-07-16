# Reason For Investigation

Checkpoint 3 Phase 4 proposed reusing a 1,024 accepted-input bound as a live-queue limit because Syndic execution-snapshot metadata is capped at 65,536 payload bytes. The investigation needed to distinguish Fjall or persisted-format constraints from Beryl-owned resource policy before that number became authoritative design.

# Outcome

Fjall 3.1.6 does not impose a 64 KiB value limit. Fjall and its resolved `lsm-tree` 3.1.6 encode key lengths through `u16` and value lengths through `u32`; the implementation accepts lengths representable by those types. Beryl-home therefore admits codec key maxima no larger than `u16::MAX` and codec payload maxima no larger than `u32::MAX` minus its four-byte record-version prefix.

The Syndic `SMALL_MAX = 65_536` metadata ceiling is a Beryl-owned bounded-read and performance policy. It is not required by Fjall, Windows, or the filesystem. Fjall uses a default 4 KiB data-block target and explicitly warns that larger keys and values have greater performance impact, but it supports values far larger than one block.

The current 1,024-entry execution-snapshot limit is also Beryl-owned. At 16 bytes per accepted-input id it consumes 16 KiB, allowing a snapshot containing the maximum 32 KiB root path plus maximum external ids and other fixed fields to remain below the chosen 64 KiB metadata ceiling. Neither 1,024 nor 64 KiB is externally mandatory. A finite bound is architecturally required for deterministic read, validation, recovery, and allocation work; its exact count and byte budgets remain design choices.

# Sources

- crates.io package `fjall` 3.1.6, checksum `9fcdc69609906151dff9b534e30eaf8515082055d36f628e382bd0b5d6a1d362`, resolved by `Cargo.lock`; root manifest enables its default `lz4` feature. Inspected registry sources `src/lib.rs`, `src/keyspace/mod.rs`, and `src/keyspace/options.rs` on 2026-07-14. Package page: https://crates.io/crates/fjall/3.1.6
- crates.io package `lsm-tree` 3.1.6, checksum `39ca67401338b98d58447387dd5230552d2241bc388206e491d137b18dfea9d6` as resolved transitively by Fjall. Inspected registry sources `src/key.rs`, `src/value.rs`, `src/table/data_block/mod.rs`, and `src/vlog/blob_file/writer.rs` on 2026-07-14. Package page: https://crates.io/crates/lsm-tree/3.1.6
- Local integration: root `Cargo.toml`, `Cargo.lock`, `crates/beryl-home-store/src/read/execute.rs`, `crates/syndic-storage/src/codec.rs`, `crates/syndic-storage/src/codec/primary/binding.rs`, `crates/syndic-storage/src/record.rs`, and `crates/beryl-model/src/runtime.rs`.
