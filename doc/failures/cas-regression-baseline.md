# Existing CAS Regression Failures At Idle-Maintenance Acceptance

## Scope And Evidence

On 2026-09-10, broad beryl-app verification ran the library tests and `connection_work`,
`runtime_demand_custody`, `process_scheduled_sessions`, `runtime_interest`,
`managed_runtime_interest` and `runtime_execution_sessions` with `test-faults` enabled.
Of 300 cases, 283 passed and 17 failed. Three runtime-interest tests counted all notifications as
preparation readiness; the new maintenance wake exposed that ambiguity. Their corrected test seam
observes the exact `ExecutionReady` bit, and all 13 runtime-interest tests pass.

The other 14 failures were reproduced from an isolated `git archive` of committed prerequisite
`df7c70b4`, using the same local owned-fork configuration, lockfile, stable toolchain and one-job
build settings. The baseline selection ran 18 cases: four passed and the same 14 failed with
matching failure details. This comparison establishes that they predate idle-maintenance changes;
it does not establish their underlying causes or excuse an unmet production guarantee.

Bounded logs and process summaries are retained under
`C:/Users/user/AppData/Local/Temp/beryl-build-memory-20260908`, with prefixes
`idle-maintenance-controls-all-20260910` and `idle-maintenance-baseline-20260910`.
Both guarded jobs reaped their root and all children. The baseline source copy, archive and
job temporary directories were verified and removed.

## Failure Families

- One active-steering delivery case expects zero total free-space observations but observes one:
  `lifecycle_before_exact_response_still_completes_delivery`.
- Five marker/replay cases fail in `tests/unit/submission_fixture.rs` at mutation-page staging
  with `MutationStaging(Invalid)`: repeated-image delivery, real image-bearing provider replay,
  both marker-aware accepted-input cases and the marker-free replay factory case.
- Five provider compaction-marker cases fail while mounting the lifecycle test operation with
  `AuthorityMismatch`, before their publication assertions.
- Three checked-user terminal-failure cases disagree with asserted health, authority or cleanup
  outcomes: before-commit terminal failure, after-persist terminal failure and persistent activation
  publication panic.

The baseline filter is:

```text
test(lifecycle_before_exact_response) | test(repeated_image_labels_dispatch) |
test(compaction_marker) | test(real_image_bearing) | test(after_persist_terminal) |
test(before_commit_terminal_failure) | test(persistent_activation_publication) |
test(input_replay::accepted)
```

Run it with `cargo +stable --config .cargo/local.toml nextest run --locked -p beryl-app
--features test-faults --lib --jobs 1 --no-fail-fast -E '<filter>'`.

## Correction Boundary

Reconcile each test against current composer, service-generation, control-custody and terminal
failure authority. Correct obsolete setup and assertions while retaining meaningful failure
coverage. Do not broaden the accepted idle-maintenance implementation or weaken runtime guarantees
to make this selection pass. A demonstrated production defect requires its own implementation
boundary. The active plan tracks this work before process composition resumes.
