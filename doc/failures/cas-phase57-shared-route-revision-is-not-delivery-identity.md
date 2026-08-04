# CAS Phase 57 Shared Route Revision Is Not Delivery Identity

## Invalid Approach

Treat the input gate and route-generation head revisions observed by a delivery worker as exclusive
preconditions for its later claim, retry, completion, rejection, or projection-loss publication.

## Evidence

Admitting another input to the same active steering generation legitimately advances both shared
revisions while leaving the first input's leaf and exact CAS target unchanged. The sibling may be
admitted after scheduler selection, after the first leaf becomes `Delivering`, or while the CAS
request is in flight. Every earlier exact-revision request then collides with valid descendant
authority. Before dispatch this can stop the scheduler; after dispatch it can misclassify known
success, exact rejection, or proven non-dispatch as delivery-unknown. Active-binding abandonment
repeated the same read-to-write race.

## Why It Fails

The shared gate and generation revisions protect aggregate state for all accepted inputs in the
generation. They are not the identity of one leaf transition. Re-reading them merely moves the
race window, and an inline retry loop has no bounded progress guarantee while admissions continue.
A process mutex would couple independent admission, delivery, lifecycle, and loss publishers and
would not provide durable replay or recovery authority.

## Course Correction

Delivery and abandonment requests carry stable operation identity: the exact input and source
leaf revision, transition or loss disposition, and exact semantic steering or binding target.
Inside the serialized Syndic mutation, storage validates that stable identity against the current
compatible generation, consumes the actual current gate and route head, updates their aggregates,
and persists those actual source facts in the successor witness. Fixed-work reconciliation proves
the same stable operation from that immutable witness and monotonic compatible descendants; it does
not require two identical reads of mutable shared authority. Sibling admission is therefore a valid
shared-authority descendant, while a changed leaf, target, binding, or disposition remains a
collision.

## Affected Authority

This correction is part of Phase 57 in `doc/plan.md`,
`doc/systems/cas-live-syndic-transcript/design.md`, `crates/syndic-storage/doc/design.md`, and
`crates/beryl-app/doc/design.md`.
