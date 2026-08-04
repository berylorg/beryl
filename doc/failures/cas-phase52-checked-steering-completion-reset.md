# CAS Phase 52 Checked Steering Completion Reset

## Invalidated Approach

The first checked delayed-steering result slot reset its sequence tracker to `Idle` as soon as it
published `Completed`. The capacity-one result still prevented another selection until consumption,
so ordinary Started-then-Completed tests passed.

## Why It Failed

Consuming the checked result is only an in-memory handoff. Phase 52 deliberately does not publish
the accepted-input delivery disposition, so consumption cannot prove that the owning route was
durably resolved. Resetting at that point admitted another `Started` lifecycle for the same
correlation and target after a fully consumed Started-to-Completed sequence.

That is an incomplete-capture invariant failure, not a new steering attempt. Accepting it could make
duplicate provider lifecycle evidence appear canonical while the original delivery still remained
`Delivering`.

## Required Course Correction

The slot retains the exact sequence proof in a terminal `Completed` state after result consumption.
Any later lifecycle fails closed and retires the production target. A later delivery phase may add
an explicit release only at the exact durable route-disposition boundary that owns the completed
result.

Regression coverage must consume both checked results before submitting the duplicate. Testing a
duplicate only while Started is pending proves capacity and mid-sequence ordering, but cannot prove
the terminal fence.
