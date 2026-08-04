# Beryl Test Hang Misdiagnosed As A Windows Cargo Stall

## Scope

Focused `beryl-app` library verification on the shared Windows Cargo target during root-plan
Phase 86.

## Invalidated Approach

The quiet interval was treated as a Windows Cargo or nextest build-path stall because early process
sampling watched only Cargo, rustc, linker, and nextest process names. Repeated bounded retries and
driver-process termination could not distinguish a build stall from a child test that never exited.

## Evidence

An ETW process trace of the exact focused command showed that Cargo metadata, the no-run test build,
test enumeration, and sequential test execution all completed normally. Every selected test exited
except
`cas_projection::service_supervisor::tests::stale_slot_validation_completes_exact_provider_waiter`.
Its test process remained alive for 1,401.8 seconds. Terminating only that exact child process caused
nextest and the outer Cargo command to exit immediately with code 1.

A symbolized context-switch trace placed the persistent-failure cut worker in
`PersistentFailureCoordinator::run_worker`, parked in `std::sync::mpsc::Receiver::recv`. Windows Wait
Chain Traversal showed the test body waiting for the recovery worker, which was waiting to join that
persistent-failure worker. There was no wait cycle and no active compile, link, disk, or test work.

The source path explains the wait. Mismatched stale-slot validation calls
`PersistentFailureNotification::elect_and_publish_stale_completion`. That path elects the master
command gate's persistent-failure owner and publishes terminal supervisor completion, but it does
not signal the persistent-failure worker. Later service settlement observes persistent-failure
ownership and calls `retain_persistent_failure`, which joins the still-parked worker without first
requesting shutdown or otherwise waking it.

The system disk was healthy, local, and had approximately 796 GB free. No relevant NTFS, storage,
resource-exhaustion, Defender, Cargo-lock, or machine-wide process evidence was present.

## Why It Failed

This was a Beryl lifecycle coordination defect, not a Cargo build stall and not a performance-sensitive
test. One stale-completion route can elect the persistent-failure cut without making the cut worker
runnable, while the retained-service route assumes an elected cut worker will eventually finish.
Those two ownership contracts are inconsistent.

## Course Correction

Do not clear Cargo artifacts, reboot Windows, update nextest, add a timeout, or classify this as a
slow test. Diagnose quiet nextest intervals by identifying the active child test and its wait chain.
Correct the lifecycle contract so every successful persistent-failure election either signals the
cut worker or transfers settlement to a path that does not join an unsignaled worker.

## Affected Work

- `doc/plan.md` Phase 86 focused and full `beryl-app` nextest verification.
- Persistent-failure election, stale verification completion, and retained-service settlement.
- Phase 86 completion review and tracker closure remain pending until the lifecycle defect is fixed
  and the required functional suites pass.
