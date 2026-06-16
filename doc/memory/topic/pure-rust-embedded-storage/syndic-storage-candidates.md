# Reason For Investigation

Operator asked which battle-tested pure-Rust embedded storage crates could support Syndic's planned turn DAG, cursor-walked turns, cursor-walked turn items, lazily loaded Markdown blocks, and fixed-buffer streaming of large generated image data.

# Outcome

The practical pure-Rust shortlist was `redb` and `fjall`.

Refresh on 2026-06-16: this exploratory note is superseded for production Syndic storage choice by `crates/syndic-storage/doc/design.md`, which chooses `fjall` as the primary engine based on later Syndic-shaped benchmark evidence. The original shortlist remains useful background for why `redb` and `fjall` were the two serious candidates.

`redb` was the lowest-risk initial default candidate because Beryl already depended on `redb = 4.1.0`, it is stable, ACID, MVCC, B-tree based, and exposes double-ended range iterators. It fits ordered metadata tables, turn/item indexes, and fixed-size chunk rows well. It should not be used as one giant appendable value store; large content should be split into chunk rows keyed by resource id and chunk number.

`fjall = 3.1.5` was the main alternative to benchmark for `redb` write amplification, large-value handling, and chunk-heavy ingestion risk. It is a pure-Rust LSM storage engine with range and prefix iteration in both directions, optional transactions, compression, background maintenance, and optional key-value separation for blob-heavy use cases. Later local benchmark results made it the production `syndic-storage` choice.

`sled` is not recommended as a new foundation despite its historical popularity. The latest crate version observed by `cargo info` was `1.0.0-alpha.124`, while docs.rs stable documentation still showed `0.34.7`; this is too unsettled for a new durable storage boundary.

`BonsaiDb` and `Nebari` are not recommended for Syndic's hot storage path because their own docs describe alpha/data-loss risk. They are also higher-level than the ordered cursor/chunk store Syndic needs.

`jammdb` is a simple pure-Rust BoltDB-like option with cursor iteration and mmap-backed B+tree reads, but it is less compelling than `redb` for this project because `redb` is already in the workspace and has stronger current positioning.

The storage shape should use composite ordered keys and opaque cursor tokens based on the last seen key:

- `turns_by_thread_view`: `(thread_id, view_order_key) -> turn_summary`
- `turn_children`: `(parent_turn_id, child_order_key) -> child_turn_id`
- `turn_items`: `(turn_id, item_seq) -> item_summary_or_inline_payload`
- `message_blocks`: `(message_id, block_seq) -> block_summary_or_inline_payload`
- `codeblock_lines`: `(codeblock_id, line_seq) -> line_chunk`
- `table_rows`: `(table_id, row_seq) -> row_chunk`
- `media_chunks`: `(resource_id, chunk_seq) -> bytes`

Forward cursors should scan from the key after the last returned key. Backward cursors should scan the bounded prefix range in reverse from the key before the first returned key.

# Sources

- Local workspace manifest, `Cargo.toml`, inspected on 2026-06-15. Existing dependency: `redb = "4.1.0"`.
- Cargo package metadata inspected with `cargo info` on 2026-06-15: `redb 4.1.0`, `fjall 3.1.5`, `sled 1.0.0-alpha.124`, `bonsaidb 0.5.0`, `nebari 0.5.5`, `jammdb 0.11.0`, `sanakirja 2.0.0-beta`, `native_db 0.8.2`, `stoolap 0.4.0`.
- `redb` docs.rs crate page, version 4.1.0, accessed 2026-06-15. Useful for pure-Rust, ACID, MVCC, copy-on-write B-tree, and feature summary.
- `redb::ReadableTable` docs.rs page, version 4.1.0, accessed 2026-06-15. Useful for `range` returning a double-ended iterator.
- `redb::Table` docs.rs page, version 4.1.0, accessed 2026-06-15. Useful for `insert`, `insert_reserve`, and table behavior.
- `fjall` docs.rs crate page, version 3.1.5, accessed 2026-06-15. Useful for pure Rust, LSM, range/prefix forward/reverse iteration, transactions, compression, and key-value separation feature summary.
- `fjall::Keyspace` docs.rs page, version 3.1.5, accessed 2026-06-15. Useful for `range`, `prefix`, `start_ingestion`, `size_of`, and insert/read primitives.
- `fjall::KvSeparationOptions` docs.rs page, version 3.1.5, accessed 2026-06-15. Useful for blob-file and separation-threshold behavior.
- `sled` docs.rs page, version 0.34.7, accessed 2026-06-15. Useful for range, tree, batch, and transaction APIs, and for noticing stable docs mismatch with latest alpha crate info.
- `BonsaiDb` docs.rs crate page, version 0.5.0, accessed 2026-06-15. Useful for alpha status and data-loss warning.
- `Nebari` GitHub README, accessed 2026-06-15. Useful for alpha status and storage-layer behavior.
- `jammdb` docs.rs crate page, version 0.11.0, accessed 2026-06-15. Useful for mmap-backed single-file B+tree, cursor iteration, and transaction model.
