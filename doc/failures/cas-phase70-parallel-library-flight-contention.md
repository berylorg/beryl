# CAS Library Fixture Projection-Flight Contention

## Invalidated Approach

Treat serialized nextest execution as sufficient synchronization for every process-local
projection-flight fixture, or treat every `ProjectionInFlight` result as a product regression.

## Evidence

The parallel Phase 70 gate failed three active-steering tests after unrelated tests had acquired
the process-wide projection-flight registry. Each failed case passed when selected alone, and the
complete 149-test library harness passed with one nextest worker.

A later serialized 252-test gate failed an active-steering fixture while that same fixture's
newly started accepted-input scheduler legitimately held the thread flight for its initial
recovered-pending pass. The exact test passed immediately in isolation because the unsynchronized
manual acquisition happened to win that time. Serialization removed cross-test concurrency but
could not order threads created inside one test.

## Why It Failed

These unit fixtures intentionally exercise process-wide flight authority. Their generated
identities are not isolated from every concurrently scheduled library test, so unrestricted
parallel execution can create unrelated cross-test admission contention. Independently, creating
`ProjectionConnectionService` starts the recovery scheduler before the fixture manually obtains a
projection. When a durable pending turn already exists, that scheduler may briefly acquire the
same flight even under `-j1`. Neither collision is evidence about the behavior the fixture intends
to exercise.

## Required Course Correction

Keep `cargo nextest run -p beryl-app --lib -j 1` as the complete library regression gate until
fixtures own collision-proof process identities or nextest process grouping. Serialization is not
a substitute for fixture-local lifecycle synchronization: a fixture that manually acquires a
projection after service construction must first observe its initial recovered-pending pass settle,
release all scheduler worker admission, and release the same-thread flight. Do not add a production
retry or weaken flight exclusivity to make the fixture pass. Focused tests may still run in parallel
when their authority domains are disjoint.

## Affected Work

Root `doc/plan.md` Phase 70 and Phase 78 verification, the active-steering delivery fixture, and
future CAS projection phases that use the shared library harness own this testing constraint.
