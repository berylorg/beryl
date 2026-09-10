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
