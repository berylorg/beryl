# Reason For Investigation

Fjall 3.1.5 cannot satisfy Beryl-home's accepted durability contract because `WriteBatch::commit` discards the fallible result from `journal_writer.write_batch(...)`. This investigation determines whether any stable, non-yanked Fjall release published after 3.1.5 through 2026-07-13 fixes that exact defect without changing the explicit batch-commit-then-`SyncAll` integration or the fail-closed forced-recovery path.

# Outcome

## Published Release Set

Crates.io reports exactly one stable, non-yanked Fjall version newer than 3.1.5 through 2026-07-13: 3.1.6.

- Fjall 3.1.6 was published at `2026-07-05T13:57:39.653416Z` and is not yanked.
- Its crates.io archive SHA-256 is `9fcdc69609906151dff9b534e30eaf8515082055d36f628e382bd0b5d6a1d362`; an independent hash of the downloaded archive matched the registry checksum.
- The archive's `.cargo_vcs_info.json` identifies source commit `80cf6bcce931a9f65dac3d0558abd02564107630`.
- The official `3.1.6` Git tag resolves directly to the same commit.
- Fjall 3.1.5 is now yanked. Its artifact remains the stable defect baseline: SHA-256 `038acd422d607e0eca09e093f299f9eccf9bd097554343d93746afff81a45113`, source commit and tag `41bc2136e5979289ba92a32797afae72fe693ab8`.

There is no 3.1.7, 3.2.x, or other later stable published Fjall version in the registry metadata. The registry's `newest_version` and `max_version` are both 3.1.6.

## Exact Defect Status In 3.1.6

Fjall 3.1.6 does not fix the defect. The extracted 3.1.5 and 3.1.6 `src/` trees are byte-for-byte identical. The official tag comparison likewise contains one release commit and changes only `Cargo.toml`: the package version changes from 3.1.5 to 3.1.6 and the `lsm-tree` requirement changes from `~3.1.5` to `~3.1.6`.

The exact affected 3.1.6 path remains:

- `src/batch/mod.rs` line 117 executes `let _ = journal_writer.write_batch(...)` and therefore discards its `Result`.
- `src/journal/writer.rs` lines 327-378 define `write_batch` as fallible and propagate failures from start, item, and end writes with `?`.
- `src/batch/mod.rs` lines 119-128 observe only the later configured persistence operation. A successful later flush does not recreate bytes omitted after an earlier transient write error.
- `src/batch/mod.rs` lines 145-162 still apply every item to memtables and publish the sequence after the discarded result.

The original failure mode therefore remains exact: a partial journal write may return an error that `commit` ignores; a later `PersistMode::Buffer` operation and Beryl's following `Database::persist(PersistMode::SyncAll)` may both succeed; Fjall can then publish and report success for a complete in-memory batch whose recoverable journal representation is incomplete.

The official repository's current unpublished `main` head inspected on 2026-07-13, commit `73a2345652eafb2604dcdf7bdd289ae70de306b7`, also retains the same discarded result at `src/batch/mod.rs` line 117. This is supplementary status only; it is not a published artifact or an acceptable dependency identity.

## Transaction Delegation And Tests

Both transaction modes remain affected:

- `src/tx/write_tx.rs` lines 311-350 converts the transaction memtables to `OwnedWriteBatch` and calls `batch.commit()?`.
- `src/tx/single_writer/write_tx.rs` lines 361-368 delegates its public commit directly to that shared transaction commit.
- `src/tx/optimistic/write_tx.rs` lines 434-454 delegates through the same shared transaction commit inside conflict arbitration.

The entire 3.1.6 test source is unchanged from 3.1.5. Existing relevant tests do not cover the defect:

- `src/db_test.rs` lines 212-243 exercises normal two-keyspace batch recovery but injects no journal write error.
- `src/journal/test.rs` exercises ordinary journal recovery and manually appended corrupt or repeating tail markers. Its direct `write_batch` calls propagate errors with `?`; it does not test the public commit path discarding one.
- No failpoint, fault injector, or downstream test utility for journal writes appears in the release source or feature manifest. `__internal_whitebox` remains limited to internal counters.

## Beryl API Compatibility And Integration Verdict

Fjall's Rust source and feature set did not change between 3.1.5 and 3.1.6, so the APIs relevant to Beryl remain present with the same behavior:

- `Database::batch` still selects `PersistMode::Buffer` when `manual_journal_persist` is false.
- `Database::persist(PersistMode::SyncAll)` remains available as Beryl's separate second-stage barrier.
- `DatabaseBuilder::into_config` and `Database::recover` remain public but `#[doc(hidden)]`, so forced same-home recovery without create-on-missing fallback remains technically expressible and remains an unstable integration surface.
- The default feature remains `lz4`; `bytes_1`, `metrics`, and `__internal_whitebox` remain disabled by default. The Rust version remains 1.90.0.
- The only dependency-manifest change relevant to a future resolution is `lsm-tree ~3.1.6` in place of `~3.1.5`. Fjall's packaged lock resolves `lsm-tree` 3.1.6 with checksum `39ca67401338b98d58447387dd5230552d2241bc388206e491d137b18dfea9d6`.

These unchanged APIs do not offset the unchanged defect. The explicit two-stage call shape still compiles in principle, and forced recovery remains available, but 3.1.6 still cannot prove Beryl's rule that no failed storage mutation is reported durably saved.

There is no technically corrected published Fjall upgrade. After this source audit, the Operator explicitly approved exact official Fjall 3.1.6 as a temporary known-risk dependency and filed [fjall-rs/fjall#304](https://github.com/fjall-rs/fjall/issues/304). That decision removes the project-plan blocker through explicit risk acceptance, not because 3.1.6 satisfies the rejected durability proof; adopting a corrected release or an owned fork remains deferred.

The root workspace now declares `fjall = "=3.1.6"` without a resolved Fjall package in `Cargo.lock`. The exact lock entry and enabled features still need verification after the permanent `beryl-home-store` consumer exists.

## Remaining Risks For A Future Candidate

A future release containing the one-line propagation correction must still be re-audited rather than accepted from a changelog claim. Verify the exact crates.io checksum and VCS identity, both transaction delegates, the ordering of commit and external `SyncAll`, hidden forced-recovery availability, complete-batch recovery, error/poison behavior, feature resolution, and permanent real-I/O fault coverage. The current upstream default branch provides no known corrected unpublished candidate as of the inspection date.

# Sources

Inspected 2026-07-13. Only primary upstream release/repository sources and current workspace authority were used.

## Crates.io Release Authority

- Fjall crate metadata and complete version list: <https://crates.io/api/v1/crates/fjall>. This established that 3.1.6 is both the newest and maximum published version and the only stable non-yanked release newer than 3.1.5.
- Fjall 3.1.6 version metadata: <https://crates.io/api/v1/crates/fjall/3.1.6>. This supplied publication time, non-yanked status, archive checksum, archive size, Rust version, download path, and features.
- Fjall 3.1.6 release page and exact archive: <https://crates.io/crates/fjall/3.1.6> and <https://crates.io/api/v1/crates/fjall/3.1.6/download>.
- Operator-filed exact defect report #304: <https://github.com/fjall-rs/fjall/issues/304>.
- Fjall 3.1.5 version metadata and exact archive: <https://crates.io/api/v1/crates/fjall/3.1.5> and <https://crates.io/api/v1/crates/fjall/3.1.5/download>. These confirmed the yanked baseline and reproduced its prior checksum.
- Both extracted release artifacts: `.cargo_vcs_info.json`, `Cargo.toml.orig`, packaged `Cargo.lock`, complete `src/`, and `README.md`.

## Official Fjall Repository

- Canonical repository: <https://github.com/fjall-rs/fjall>.
- Fjall 3.1.5 source commit: <https://github.com/fjall-rs/fjall/commit/41bc2136e5979289ba92a32797afae72fe693ab8>; official tag `3.1.5` resolves to this commit.
- Fjall 3.1.6 source commit: <https://github.com/fjall-rs/fjall/commit/80cf6bcce931a9f65dac3d0558abd02564107630>; official tag `3.1.6` resolves to this commit.
- Exact tag comparison: <https://github.com/fjall-rs/fjall/compare/3.1.5...3.1.6>. It contains only commit `80cf6bcce931a9f65dac3d0558abd02564107630` and changes only `Cargo.toml`.
- Defective batch path at the 3.1.6 commit: <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/batch/mod.rs#L94-L180>.
- Fallible journal writer at the 3.1.6 commit: <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/journal/writer.rs#L327-L378>.
- Shared transaction commit and delegates at the 3.1.6 commit: <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/tx/write_tx.rs#L311-L350>, <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/tx/single_writer/write_tx.rs#L361-L368>, and <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/tx/optimistic/write_tx.rs#L434-L454>.
- Two-stage persistence and forced recovery APIs at the 3.1.6 commit: <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/db.rs#L146-L198>, <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/db.rs#L329-L370>, <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/db.rs#L384-L416>, <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/db.rs#L567-L590>, and <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/builder.rs#L23-L63>.
- Relevant unchanged tests at the 3.1.6 commit: <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/db_test.rs#L212-L243> and <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/journal/test.rs#L57-L487>.
- Current unpublished main-head commit inspected for supplementary status: <https://github.com/fjall-rs/fjall/commit/73a2345652eafb2604dcdf7bdd289ae70de306b7>; its unchanged defective line is <https://github.com/fjall-rs/fjall/blob/73a2345652eafb2604dcdf7bdd289ae70de306b7/src/batch/mod.rs#L117>.

## Workspace Authority

- `Cargo.toml`: Fjall workspace dependency declaration.
- `Cargo.lock`: no resolved Fjall package at inspection time.
- `doc/failures/fjall-3.1.5-batch-write-error-loss.md`: exact invalidated approach and dependency gate.
- `doc/memory/crates.io/fjall/3.1.5/home-store-atomicity-and-durability.md`: baseline artifact identity, defect proof, two-stage persistence analysis, transaction delegation, forced recovery, and fault-test constraints.
- `doc/systems/beryl-home-storage/design.md` and `crates/beryl-home-store/doc/design.md`: accepted atomic commit, `SyncAll`, fail-closed recovery, and verification contracts.

## Reproduction Commands

- Query release completeness and identities with `Invoke-RestMethod -Headers @{ 'User-Agent' = 'beryl-fjall-release-audit/1.0' } -Uri https://crates.io/api/v1/crates/fjall` and the exact `/3.1.5` and `/3.1.6` endpoints.
- Download each exact archive from its crates.io download endpoint and verify it with `Get-FileHash -Algorithm SHA256`.
- Extract both archives and run `git diff --no-index --exit-code -- fjall-3.1.5/src fjall-3.1.6/src`; this exited zero.
- Run `git diff --no-index --stat -- fjall-3.1.5 fjall-3.1.6`; only `.cargo_vcs_info.json`, generated/package manifests, and packaged `Cargo.lock` differ.
- Resolve official tags with `git ls-remote --tags https://github.com/fjall-rs/fjall.git`; `3.1.5` and `3.1.6` matched their archive VCS commits exactly.
- Inspect the official GitHub comparison API at `https://api.github.com/repos/fjall-rs/fjall/compare/3.1.5...3.1.6`; it reports one commit and one changed repository file, `Cargo.toml`.
- Search the exact 3.1.6 release source with `rg -n 'write_batch|pub fn commit|fn commit|SyncAll|recover' src` and `rg -n -i '\b(failpoint|fault injection|fault injector|test_utils)\b' src Cargo.toml.orig README.md`.
- No manifest was edited, no dependency was resolved into the workspace, no software was installed, and no compilation was needed to establish the source-equivalence verdict.
