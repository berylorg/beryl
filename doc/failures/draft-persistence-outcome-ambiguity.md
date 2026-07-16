# Draft Persistence Outcome Ambiguity

## Invalidated Approach

The first Checkpoint 3 plan required every storage or cancellation failure during draft persistence to leave the durable draft unchanged.

That promise treated an error returned to the caller as proof that the database still contained the old state, and treated cancellation as if it could retract work after the serialized writer had admitted it.

## Why It Failed

The Beryl-home writer observes cancellation only before writer admission. Once admitted, the command must finish so that one atomic batch and its persistence barrier have a definite internal ordering.

A storage or persistence failure may surface after the batch reached the database. A crash at the commit and persistence boundary may likewise reopen with either the whole old state or the whole new state. Neither case permits the caller to infer rollback merely from the missing success receipt.

## Course Correction

Validation rejection, revision conflict, and pre-admission cancellation retain the old durable draft exactly.

After writer admission, cancellation no longer retracts the save. Lifecycle flushes drain admitted work. A surfaced storage or persistence failure is classified as an ambiguous durable outcome: Beryl retains the local editor payload, gates success and dependent lifecycle actions, verifies or recovers the same home, reacquires the current healthy generation, and coherently rereads the exact current draft identity, revision, and payload.

Reconciliation accepts only the whole old or whole new atomic state. It never guesses, blindly retries an ambiguous natural identity, merges unexplained revisions, creates a second current draft, or discards the local editor payload.

## Resolution

The Operator accepted this correction on 2026-07-14. Root plan, Beryl-home system, composer feature, `beryl-app`, `syndic-storage`, and rework tracking authority now use the corrected boundary.
