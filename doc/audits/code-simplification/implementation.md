# Accepted Implementation Outcomes

This record tracks later implementation against the completed audit's frozen baseline. It does
not change the baseline estimates, count overlapping findings twice, or act as another plan.

## AP-007 And AM-007: Host Request Sequence

Accepted on 2026-09-07 in Phase 305. AP-007 is implemented; AM-007 is resolved by the same
correction. Both retain their baseline zero-line estimate and shared `host-request-high-water`
overlap group. No line-reduction credit is claimed.

The host owns the checked request high-water mark. Mounted text, object, historical-object and
successor-proof requests obtain their next ID from that host and admit it under exclusive
synchronous access. Editing, history adoption, publication and same-session rebinding preserve
the sequence. Exhaustion cannot wrap; disposal followed by fresh activation establishes a new
generation before numbering restarts. The widget mutation/history allocator remains separate.

Production changes are in `crates/beryl-app/src/composer_host/{request.rs,history.rs,
mutation/settlement.rs,lifecycle/flush.rs,lifecycle/service.rs}` and
`crates/beryl-app/src/main_window/{composer_slot.rs,composer_slot/dispatch.rs,
composer_slot/dispatch/proof.rs,composer_slot/retirement.rs,composer_slot/submission.rs}`.
Three new behavior tests are in
`crates/beryl-app/tests/main_window_composer_slot/request_sequence.rs`, registered by its parent
target. Independent semantic review inspected all allocation sites, admission serialization,
binding/generation changes, errors, cancellation, disposal and the final tests without a blocking
finding. Root validated the source diff, the generation-reset assertions and verification evidence.

## Verification

The focused direct run `8bd3fe36-4025-40a1-8464-6b668b2bd259` passed five cases: three new
request-sequence cases and existing activation-replay and stale-completion/cancellation cases.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --test main_window_composer_slot --test syndic_composer_host -E 'test(request_sequence) | test(activation_replay) | test(stale_completion)' --test-threads 2 --no-fail-fast
```

The resident-close run `9be8128e-8daa-496f-bbcb-9aeae8945058` passed
`pending_and_ready_close_preserve_resident_interaction_and_failure_releases_only_its_gate`,
including publication rebinding, selection/copy, failure-release, undo/redo and later editing.
The mounted-submission run `3e2b76d8-297c-4868-96dc-9f027ad3665e` passed
`mounted_collision_sets_unavailable_only_after_host_ticket_settles_without_custody`, which
previously timed out. The submission scheduler was unchanged.

Both mounted invocations used `RUST_MIN_STACK=33554432`, scoped to the child process and restored
afterward, plus a temporary nextest configuration with
`slow-timeout = { period = "30s", terminate-after = 1 }`:

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/host-request-sequence-nextest.toml --test resident_close_flush -E 'test(pending_and_ready_close_preserve_resident_interaction_and_failure_releases_only_its_gate)' --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/host-request-sequence-nextest.toml --test mounted_composer_submission -E 'test(mounted_collision_sets_unavailable_only_after_host_ticket_settles_without_custody)' --test-threads 2 --no-fail-fast
```

The temporary configuration was removed after verification. The locked app library check passed:

```powershell
cargo +stable --config .cargo/local.toml check -p beryl-app --lib --locked
```

Targeted `rustfmt +stable --edition 2024 --config skip_children=true --check` and scoped
`git diff --check` passed across the 12 changed source/test files; root's whole-worktree
`git diff --check` also passed. No manifests, dependencies or lockfiles changed.

## Verification Limits And Cleanup

Initial placement in `main_window_composer` and `syndic_composer_publication` encountered 20 and
110 pre-existing compiler errors, chiefly obsolete storage-handle ownership assumptions. The new
tests were moved into the compiling slot target, retaining their assertions and using existing
behavior support. Those broader targets were not repaired or certified.

Bounded mounted run `8a520ebf-7582-4b7e-86dd-cb8d3108b716` failed two existing marker-count
preconditions before publication (`0` rather than `1`, mount-test lines 2301 and 2411) and aborted
the close case on the default test-thread stack. The larger-stack close retry passed. Neither
marker failure supplied a request-identity error; their scenarios remain unaccepted. This work
does not establish full Phase 302, 306, 307 or 318 acceptance. The earlier 39 passing saved-opening,
lifecycle and native-lineage cases remain a separate unaccepted integration checkpoint.

All Cargo/test sessions completed and process inspection found no matching running binaries or
aborted-test PIDs. The temporary timeout configuration and empty relocation directory were removed.
Automatic approval review rejected deletion of
`C:\Users\user\AppData\Local\Temp\mounted-readonly-4IjvPw` with `blocked by policy`.
That aborted-test home remains (602 files, 72,102,806 bytes); no alternate deletion mechanism was
attempted after the exact-path rejection. This cleanup limitation does not change the functional
verification result and must remain explicit at handoff.

The Operator subsequently authorized temporary and obsolete directory removal for the remainder
of the thread. A guarded retry against that exact path was still rejected by automatic approval
review with `blocked by policy`; the directory remains. This is a tool-policy restriction rather
than missing Operator authorization, and it does not block independent implementation work.

## AM-008: Distinct Mounted-Test Support Modules

Implemented and accepted on 2026-09-07 in Phase 307. The native-lineage helper module now uses
the behavior-named `native_lineage_support` alias, while slot fixture support retains `support`.
Both helpers and their consumers remain; the mounted target compiles with `test-faults`.
The baseline correctness estimate remains zero, with no line-reduction credit claimed.

The [opening integration evidence](../../failures/pristine-editor-publication.md) records the
52-case integrated pass, including four selected native-lineage cases, targeted formatting and
independent source/test review. Nine other mounted scenarios were excluded from that focused run;
this wiring correction does not certify their behavior or accept later close phases.
