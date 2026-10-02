# Crash Report Test Containment

The independent reporter tests initially ran through `cargo nextest` on Windows. The reporter
remained in a job after requesting breakaway and correctly refused readiness; a direct fixture
launch succeeded. Requesting breakaway for the test producer merely moved the failure to an
access-denied reporter launch. Nested-job behavior must not be mistaken for channel corruption
or repaired by weakening production readiness checks.

Invoke the installed `cargo-nextest.exe nextest run` directly for these process tests, retaining
the ordinary locked Cargo build configuration. This removes the extra Cargo launch containment.
All 17 focused tests passed on 2026-09-10 with unchanged production launch behavior, including
explicit job refusal and exact provisional-child timeout reaping. No extra launcher, dependency,
test skip or job-policy mutation is needed.

The [crash-reporting system](../systems/crash-reporting/design.md) still requires abort-only
fallback when the actual application environment prevents reporter independence.

## Executable Mount Acceptance

Accepted on 2026-10-03 after reconstructing ordinary executable bootstrap. The unchanged helper
baseline again demonstrated the documented nested-job refusal when invoked through Cargo
(`adba921f-6f0e-4b3e-a260-a5be34862492`); both receivers reported `in_job=true`. Direct installed
nextest execution resolved the test containment issue without weakening production readiness,
adding a launcher, or changing job policy.

The actual executable now has process evidence for malformed reserved reporter dispatch,
successful Running shutdown releasing its one waiting reporter, and a real opener panic before
home creation. The last case retains the exact reporter handle while the parent is alive, releases
the bounded test gate, observes failed parent exit, then observes and closes the independently
owned native report window. Existing report capture, transport, clipboard and two-command surface
evidence remains applicable. The fault trigger exists only under the binary's `test-faults` feature.

Review corrected a test-only process-discovery race: an initial snapshot's PID alone could not
prove the identity of a subsequently opened same-image process. Adoption now revalidates the
parent relation against a fresh bounded snapshot while exact parent/candidate handles remain live,
before granting cleanup ownership. The final normal-exit test waits for Running and requires
successful exit rather than accepting startup cancellation.

Canonical locked metadata, feature-enabled all-target checks and the ordinary default binary
check passed without local dependency overrides. Direct nextest run
`7718d3bc-3b15-4e88-9d58-f41f08e28830` passed all 13 executable mount/bootstrap cases in 8.890
seconds. Independent review accepted entry ordering, test isolation, exact process ownership and
actual window evidence, with all six changed source/manifest/lock hashes matching the qualified
source. Bounded evidence remains under `.tmp/crash-mount-evidence`. The isolated checkout was
removed and no owned application, reporter or test process remained; unrelated installed Beryl
instances were untouched.
