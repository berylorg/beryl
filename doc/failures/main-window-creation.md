# Main-Window Creation Progress

## Invalidated Approach

Phase 290 initially stopped scheduling a pending creation after four continuations. Exact custody
remained retained, but no ordinary event resumed that request after temporary home-revision
contention ended. Candidate opening can return `NotCommitted`, then `Retry`, then creation
`Pending`; the fifth such outcome left New Window disabled indefinitely before hidden-window
construction and its readiness timeout.

## Accepted Correction

The process creation owner retains one continuation timer per pending entry with backoff capped at
800 ms. A 30-second deadline begins at admission and cancels publication, while exact retirement,
abandonment, and reconciliation continue to settlement with their original reservation. Cancellation
never becomes evidence of noncommit or permits a replacement operation identity.

The six-conflict regression makes six distinct durable placement changes immediately before
candidate-open commits, then allows the same creation to publish. This exercises actual home
revision conflicts beyond the old cutoff. Related tests cover cancellation after delayed worker
completion, indeterminate acquisition, actual native visibility failure, deadline cleanup, exact
runtime/root capture, independent focus, and four creation/release cycles returning registry and
appearance counts to their baseline. Raw OS removal in those tests is fixture release, not product
ordinary-close implementation.

Independent semantic review accepted the final creation and publication boundary. It also verified
supported command theme roles and live tooltip projection: a retained tooltip reads the invoking
root's current reason and appearance instead of keeping a stale copied generation.

## Verification

The affected nextest run on 2026-09-06 covered `phase290_main_window_creation`,
`phase236_window_acquisition`, `phase238_window_abandonment`, `phase295_initial_composer`,
`phase289_main_window_shell`, and `main_window_reservations` in `beryl-app`, with `test-faults`,
`--locked`, serial execution, and a 60-second per-test termination limit.

Aggregate run `4520642c` passed 62 of 64 cases. The deadline and worker-cancellation cases failed
while provisioning their Syndic test stores with `OpenKeyspace` OS code 5 (`PermissionDenied`),
before reaching creation behavior. The targeted rerun `dd2fa392` passed both cases. Every affected
case has passing evidence, but there was no single clean 64-case aggregate. An earlier occurrence
of the same fixture-only error also passed its single targeted rerun; no production storage
workaround was introduced.

The final `cargo check -p beryl-app --lib --locked`, scoped Rust formatting check, and scoped Git
diff check passed. The feature-enabled library check also passed during implementation. All 12
new focused cases have passing results. Executable startup, ordinary close, Exit, and other
deferred mounts remain outside this phase.

## Retained Test Resources

Automatic approval review rejected recursive deletion of the task-owned directory
`C:\Users\user\p\berylorg\beryl\.tmp\creation290-20260906` with the stated reason
`blocked by policy`. The command did not execute, and no deletion retry or child-path workaround
was attempted.

The retained root contains 1,676 files totaling 1,078,950 bytes and no reparse points. Its top-level
entries are `.tmp5tIr3t`, `.tmp6ZNcs2`, `.tmpYzmR6b`, `.tmpz5FhKO`, and `nextest.toml`: test-store
fixtures and the bounded nextest timeout configuration. No task process remains running, and
process-local stack, TMP, and TEMP settings were restored. Leave this denied cleanup target
untouched unless the Operator explicitly changes that disposition; earlier denied Phase 296 and
Phase 288 resources were not touched.
