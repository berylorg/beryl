# Windows Cargo Nextest Build Stall

## Scope

Focused `beryl-app` library verification on the shared Windows Cargo target during root-plan
Phase 86.

## Invalidated Approach

Repeatedly rerunning the same focused nextest selection after the interrupted Phase 86 draft was
formatted, split, and proven by `cargo check` was expected to reach test enumeration or produce a
compiler failure.

## Evidence

The pre-split focused selection completed 13/13 tests. A later supervisor-only nextest build then
exceeded 120 seconds without test output. After the source split, the exact combined focused
selection stalled again under 300-second and verbose 600-second bounds before any `rustc`, linker,
or test process appeared. Only the task-owned Cargo and cargo-nextest driver processes remained,
with near-zero CPU, until their exact process ids were stopped and verified gone.

In contrast, `cargo fmt --all -- --check` passed and
`cargo check -p beryl-app --lib --features test-faults -j 1` completed after the split in 9.58
seconds with warnings only. The stall therefore supplies no product-test verdict and no evidence
that the Phase 86 production wiring failed to compile.

Later bounded retries proved the failure is intermittent rather than a permanently invalid test
selection: several invocations compiled and entered the focused suite, exposing ordinary functional
failures that were corrected in turn. After the broker acknowledgement boundary was corrected to
release its outer drain-counted permit before publishing completion, two consecutive focused
retries again produced no compiler or test output for approximately 128 to 133 seconds. Each retry
was stopped by exact process id and left no Cargo, rustc, or nextest process behind.

## Why It Failed

The exact cause inside the Windows Cargo/nextest build path is not yet established. Repeating the
same command does not produce additional diagnostic evidence and risks leaving owned driver
processes behind. Clearing shared target or cache state would be an unapproved workaround and could
destroy unrelated build state.

## Course Correction

Stop retrying after the reproduced no-output stall. Terminate only the exact task-owned processes,
verify cleanup, retain the compile and earlier focused-test evidence without treating it as phase
acceptance, and record Phase 86 as blocked on its required nextest gate. Do not clear shared Cargo
artifacts or substitute another test runner without Operator direction.

## Affected Work

- `doc/plan.md` Phase 86 focused and full `beryl-app` nextest verification.
- Phase 86 independent completion review and tracker closure remain unavailable until the required
  test gate can run.
