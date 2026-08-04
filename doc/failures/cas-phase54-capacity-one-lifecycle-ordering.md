# CAS Phase 54 Capacity-One Lifecycle Ordering

## Invalidated Approach

Retain only one checked delayed-steering lifecycle result while the delivery owner waits for the
exact `turn/steer` response.

## Evidence

The pinned protocol permits matching `item/started` and `item/completed` notifications to arrive
before the exact response. In the mounted ordering test, `Started` occupied the result slot and
the immediately following `Completed` was rejected as capacity full before the delivery owner
could observe the response and consume either result.

## Why It Failed

Capacity one bounded memory, but it did not bound the complete protocol sequence. It accidentally
made correctness depend on the response overtaking `Completed`, even though response and
notification ordering are independent.

## Architectural Correction

The checked tracker owns a strictly bounded ordered two-result buffer, exactly large enough for
one matching Started-to-Completed sequence. Sequence proof still rejects duplicates, mismatches,
and any third result. The delivery owner consumes Started then Completed and releases the terminal
tracker only after durable route disposition.

## Reusable Lesson

A bounded handoff must cover the maximum valid protocol burst before its consumer can run.
Choosing capacity from the usual scheduling order turns legitimate event reordering into a false
resource failure.
