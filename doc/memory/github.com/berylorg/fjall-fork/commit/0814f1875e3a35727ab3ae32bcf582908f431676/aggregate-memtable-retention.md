# Reason For Investigation

The original same-service app input qualification reached Fjall's 256 MiB retained-memtable limit
while preparing the first 128-image draft. Determine whether active data, stalled maintenance or
retained backing caused the refusal without changing the workload or limits.

# Outcome

On 2026-09-15, two instrumented runs of the original workload isolated a completed flush still
retained by eligible version history. At refusal, physical payload was 268,434,712 bytes across
101,159 records; latest installed logical payload was 201,321,606 bytes across 87,592 records.
One prospective batch-prefix record reserved 2,398 bytes. The exact difference, 67,110,708 bytes
and 13,566 records, equals the completed `_beryl_domains` flush.

The flush observed GC watermark 13,741 and visibility 13,745. At refusal, there were zero live
snapshots, visibility was 18,176, the watermark was 17,870, and that keyspace retained three
versions. All 115 keyspaces had zero latest sealed tables; all observed rotation/flush generations
were complete and inactive. There was no maintenance terminal, and only two journal files existed.
The largest active keyspace held 55,201,186 bytes, below its 67,108,864-byte rotation threshold.

Flush publication retains a predecessor under its pre-publication watermark. Later snapshot
closure advances the tracker but does not invoke version-history retirement. Retirement occurs
on later rotation or version-publication work; compaction's `DoNothing` branch does not perform
it. An eligible predecessor can therefore keep the flushed table physically charged even when
the latest version has no sealed tables. This establishes a retained history owner, not exclusive
ownership of every backing reference.

The concrete next boundary is bounded snapshot-safe history retirement under physical memtable
pressure before journal admission. Preserve real snapshot/backing pins and direct typed refusal
when reclamation cannot provide capacity. There is also no aggregate-pressure rotation trigger,
but the observed failure is not solely active-table accumulation; reassess rotation only after
eligible history retirement. Beryl's plan and failure record own the resulting work decisions.

# Sources

- Fjall path dependency version 3.1.6, canonical repository
  `https://github.com/berylorg/fjall-fork`, commit
  `0814f1875e3a35727ab3ae32bcf582908f431676`, inspected 2026-09-15. Relevant files:
  `src/db_config.rs`, `src/batch/mod.rs`, `src/keyspace/options.rs`,
  `src/keyspace/maintenance.rs`, `src/supervisor.rs`, `src/flush/worker.rs`,
  `src/snapshot_tracker.rs`, and `src/maintenance/completion.rs`. Temporary bounded test-fault
  probes in batch preparation and committed flush reporting were removed; both files again match
  the exact commit.
- Beryl `Cargo.toml`, locked `Cargo.lock`, and
  `crates/beryl-home-store/src/store/profile.rs` select this path dependency and the 256 MiB
  physical payload / 1,000,000-record limits. Runs used Windows MSVC, `test-faults`, one Cargo job,
  LLVM linking, no normal debug information and no incremental compilation.
- The nested local LSM-tree version 3.1.6 has HEAD
  `e6478e3c93fe6439b25b3c58d2de181c86cef039` with pre-existing working changes. The findings bind
  those actual sources, not HEAD alone. SHA-256 identities of the relevant source snapshot:

```text
src/memtable/residency.rs 20CE5369DD0FDF92F7A1B57A1B3F8083C505AF8E72B8D6FB5A658270B517C133
src/memtable/mod.rs DF5B12E256F372B8818E8F18457F1265CFEF788C6A2DED04A61BD9432E98E10F
src/key.rs 3E6B73F5D91B5957D4F60C38E6B9B8A2B6392E3C9495E3320CB90C025EAD2F6B
src/version/super_version.rs 3B8A8A57E9554624924EAF51F69006B448924F75A7EFE47A8EE6865D1DD0A1FF
src/tree/mod.rs A9A8D473A156A8312FAC725CD40398023085E424A7CFDA59791E54C30047BCB3
src/compaction/worker.rs 3387C422C18F7A15CFBC3C68BD749DF2909F5CE67FF798D654280D048715653E
src/tree/sealed.rs 058F6FAC6437B7F95EB99DDC4201AD859F0E63F6E4F9B220BB2C79130A3BC4D3
```

- Runtime command: `cargo nextest run -p beryl-app --test submitted_input_residency --features
  test-faults --locked -E 'test(submitted_input_logical_work_scales_and_local_capacity_releases)'
  --test-threads 1`. Runs `d363c301-9e5b-4758-ac45-ce6e0bb9140f` and
  `d58a5667-4f1b-4b48-ae11-49d08777aa4c` reproduced the safe refusal after 151.325 and 148.027
  seconds. The first captured aggregate/per-keyspace accounting; the second added exact flush,
  snapshot, journal and history-length metadata. Independent source and evidence review accepted
  the diagnosis. These expected failures are diagnostic evidence, not qualification passes.
