# Persistent Tree Balancing Qualification

Accepted on 2026-10-22 after independent semantic review and focused persistence verification.
The [draft storage contract](../../crates/syndic-storage/doc/design-draft-storage.md)
and [canonical schema](../../crates/syndic-storage/doc/design-schema-v7.md) remain authoritative.

One private packing implementation serves sequence, identity and marker-order insertion and
marker underflow repair. Sibling merge/redistribution is shared between the identity/order trees.
Identity/rank traversal, authenticated loading, summary constructors, root validation and collapse
remain separate. The 128-child threshold, midpoint, child ordering, left-before-right constructor
evaluation, immediate failure propagation and charged output are unchanged. No durable format or
public API changes.

## Verification

Canonical checkout, shared target, locked offline dependencies and one serial Cargo lane:

- `cargo nextest run -p syndic-storage --test marker_continuation_bounds --test piece_tree --test draft_root_marker_summary --test durable_builder --features test-faults --locked --offline --target-dir C:/Users/user/p/berylorg/beryl/target --test-threads 1 --status-level fail --final-status-level fail`
  passed all 58 cases in 222.121 seconds; run `5b62623e-844c-4bb3-9992-6606526eefc8`.
- `cargo check -p syndic-storage --lib --no-default-features --locked --offline --target-dir C:/Users/user/p/berylorg/beryl/target`
  passed in 11.54 seconds.

The cases cover complete merge/redistribution and retained leaf-root wrappers, sparse heights
45/63/64, exact acquisition/emission/byte bounds, reservation before retained output, split and
removal reshaping, physical reopen, canonical vectors, authenticated commitments, substituted-root
refusal and durable continuation/publication. An initial compile caught a stale exhaustive fixture
match after the earlier accepted-input replacement variant was added. Its explicit unexpected-case
panic now matches the existing ordinary-mutation fixture contract; no assertion was relaxed.

Independent source review found unchanged constructor vectors/order, canonical identities,
failure charging, bounded allocations and root authentication, with no blocking findings.
This is functional/persistence qualification, without sustained performance or global RSS claims.

## Input And Receipt Identities

The four changed inputs matched the canonical checkout:

- `persistent.rs`: `FC82420D1FFDAF7DF2E85DDFC7AEDFD45C134FF50BEA070EA98F360919398F96`.
- `persistent/marker_tree_edit.rs`: `08FA5F2677D1BE51D05114C2A6303BE4F743F94A98451EA6650CDDCD2BB50577`.
- `persistent/tree_balance.rs`: `58723348BFBFD0F99D01431B74247197E6C3171030C6071A30C61EBAF8F767D2`.
- `tests/piece_tree.rs`: `0A3160D843DDFE1118B5F2554A2C5A498D97F0A428D344C0828F377599AAFCA5`.

Source paths above are under `crates/syndic-storage/src/draft_piece/` except the test path,
which is relative to `crates/syndic-storage/`.
Test receipt SHA-256: `FEDA680322C8EABBA5D8FD687CBA81640219E725531189663318ACABBD7A99BA`.
Production receipt SHA-256: `6960433B820274D9A5D977F3E26032F6DED0867A7C026E25B9FB13F83CE22026`.
