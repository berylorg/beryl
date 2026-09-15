# Reason For Investigation

The accepted history-retirement correction allows the first 128-image app input, but the repeated
128-image input reaches the same physical payload limit. Identify the remaining charged owner and
whether maintenance is pending, without changing workload, limits or service lifetime.

# Outcome

Two bounded metadata-only captures reconcile all 115 keyspaces. Latest logical charge equals active
charge: 268,423,612 bytes / 122,859 records. Metadata charge is zero. The prepared batch prefix is
4,967 bytes / four records, exactly the difference to physical charge of 268,428,579 bytes / 122,863
records. There are no latest sealed tables, every history has one version and live snapshot count
is zero. All observed maintenance is idle; the second capture includes the distinct Rotate state:
113 keyspaces have requested/completed 0/0 and two have 1/1, with no queue, active work or waiters.
Metadata progress is 115/115 and idle. There is no maintenance terminal.

Largest active payloads are `_beryl_domains` 66,771,824 bytes, candidate sessions 63,837,054,
draft build progress 50,399,670, draft builds 15,883,506 and draft nodes 15,801,807. Every keyspace
is below the fixed 67,108,864-byte rotation threshold. The batch requests 33,552 bytes / 12 records
against 11,844 bytes of initial headroom. Its rejected next record would add 15,771 bytes after the
four-record prefix. Active charge counts internal sequence records and tombstones, not only
current application-visible data.

The first two retirement passes release 67,110,708 and 67,111,708 bytes; the final pass releases
zero. Thus the prior correction works, while aggregate active-memtable accumulation causes this
later refusal. Samples are sequential, not an atomic ownership census: exact reconciliation and
idle maintenance justify the capacity conclusion without claiming backing aliases cannot exist.

There is a separate progress constraint on any proposed pressure flush. Dependency publication
assigns replacement version sequence `S` and advances visible sequence to `S + 1`. With no snapshots,
Fjall GC reaches `S`; history retirement selects the newest version strictly below GC and preserves
that version. It therefore cannot yet retire the flushed predecessor using the new version at `S`.
A failed application preparation allocates a sequence but publishes no visibility, so retrying it
does not resolve this fence. Flushing plus idle waiting alone is not an established remedy.
Beryl's failure record and future owning design must resolve the correction; this note defines
neither a new watermark algorithm nor aggregate-maintenance policy.

# Sources

- Canonical repository `https://github.com/berylorg/fjall-fork`, commit
  `a035895a5cb694bcbfb53d0ae68c8bcd1bd7f5b1`, Fjall 3.1.6, inspected 2026-09-15. Beryl consumes it
  through its root path dependency and existing locked resolution. Inspected batch preparation,
  `src/supervisor.rs`, `src/snapshot_tracker.rs`, `src/keyspace/maintenance.rs`, fixed keyspace
  options, metadata wrapper and maintenance progress. Temporary `test-faults` probes in batch,
  supervisor and metadata source were removed; those files again exactly match the commit.
- Nested LSM-tree HEAD is `e6478e3c93fe6439b25b3c58d2de181c86cef039` with pre-existing working
  changes, unchanged by this diagnosis. Its relevant actual-source SHA-256 identities are:

```text
src/version/super_version.rs 3B8A8A57E9554624924EAF51F69006B448924F75A7EFE47A8EE6865D1DD0A1FF
src/memtable/residency.rs 20CE5369DD0FDF92F7A1B57A1B3F8083C505AF8E72B8D6FB5A658270B517C133
src/tree/mod.rs A9A8D473A156A8312FAC725CD40398023085E424A7CFDA59791E54C30047BCB3
```

- Runtime command: `cargo nextest run -p beryl-app --test submitted_input_residency --features
  test-faults --locked -E 'test(submitted_input_logical_work_scales_and_local_capacity_releases)'
  --test-threads 1`. Windows MSVC, LLVM linker, one Cargo job, no normal debug information or
  incremental compilation. Runs `9e736b80-6a2d-4c4a-bac1-6e96220594d7` and
  `a311d92d-2f78-4e70-b67c-a0da0c874761` failed as expected after 238.516 and 230.852 seconds;
  compilation took 47.23 and 45.82 seconds. The second run adds Rotate progress. GC/visible are
  27,200/27,201 in the first run and 27,201/27,202 in the second; all charge totals match exactly.
  Independent read-only review verified both captures and the source interpretation. Neither run
  qualifies the app workload or validates a future production remedy.
