# Windows Parallel Rust Link Artifact Contention

## Scope

Concurrent Cargo/nextest verification for large Beryl packages on the shared Windows target
directory.

## Invalidated Approach

Phase 72 allowed the app and storage package suites to compile and link concurrently while both
agents shared the same target directory and system drive.

## Evidence

The app package run failed during linking with `no space on device` plus Windows linker PDB errors
`LNK1108` and `LNK1318`. A later unpartitioned storage run failed with Windows OS error 112.
Multiple Cargo and nextest processes were active concurrently, and 442 abandoned
`%TEMP%\beryl-syndic-*` test homes occupied about 28.7 GB before their validated cleanup.

Phase 82 later reproduced the storage side of the failure with a serial, single-worker
`beryl-app` library gate. Two interrupted runs left 183 `tempfile`-named `%TEMP%\.tmpXXXXXX`
Beryl homes containing the exact `home.lock` plus `state` layout. They occupied about 7.8 GB even
after a fresh no-debug-symbol Cargo target remained near 2 GB, and later tests consequently failed
only with Windows error 112 / Fjall `StorageFull`.

## Why It Failed

Concurrent Rust test linking can temporarily multiply object, executable, incremental, and PDB
artifacts, while failed or interrupted storage tests can leave large temporary homes behind. Their
combined pressure can exhaust the shared drive or contend on linker outputs. Those failures do not
classify product behavior and can obscure real test results.

Serial execution prevents linker contention but does not bound abandoned test-home residency. A
large app library run can still exhaust a constrained drive unless it is partitioned and its exact
run-owned temporary homes are reclaimed between partitions.

## Course Correction

Run large Beryl package suites serially and partition storage runs on this Windows workspace; keep
the established single-worker app library gate where required. After an interrupted storage run,
validate and remove only its exact disposable `beryl-syndic-*` temporary homes. Do not delete
target artifacts merely to mask concurrent verification pressure.

On app-library runs that use `tempfile`, also record the run start, require every cleanup candidate
to remain directly under the resolved user temp root, match the exact `.tmpXXXXXX` leaf shape, and
contain the Beryl-home `home.lock` plus `state` layout. Remove only candidates created by that run,
only after confirming no Cargo, nextest, Rust compiler, or app test process remains. Partition the
suite so this validated cleanup can occur before the next group.

## Affected Work

- `doc/plan.md` Phase 72 verification
- `doc/plan.md` Phase 82 verification
- Future multi-package Rust verification using the shared Windows target directory
