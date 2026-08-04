# CAS Phase 70 Late Hard-Attachment Authority

## Invalidated Approach

Creating the process-local hard-stop slot only before the primary `turn/interrupt` response was
initially treated as sufficient because a direct hard-stop caller can attach before dispatch and
continue inline on the foreground driver.

That boundary is invalid. The status-line contract keeps hard escalation separately available
while an accepted soft stop remains stopping, and the CAS lifecycle contract requires escalation
to attach to the same durable stop without repeating the primary interruption. The initial state
machine also collapsed a confirmed `RequestAccepted` response together with completion-unknown
dispatch into `PossiblyDispatched`, leaving no safe authority for a later hard continuation.

## Correction

Confirmed primary acceptance is represented by its own process-local state. A first hard caller may
atomically freeze and own the once-only hard continuation either before primary settlement or after
confirmed acceptance while the same exact foreground connection remains authoritative. The latter
path queues only the hard continuation on that surviving driver, freshly reauthorizes and binds the
exact target, never sends another `turn/interrupt`, and lets duplicates join one immutable result.

The primary-settlement transition and the decision that no hard slot exists must occur under one
coordinator-state lock. Publishing accepted primary state and only later checking or creating the
slot is invalid: a hard caller can attach between those operations, become the sole late owner, and
then be stranded when settlement independently concludes that no continuation was attached.

Completion-unknown, authority loss, terminal-first consumption, and safe-nondispatch-first
consumption cannot synthesize this late authority. The distinction remains deliberately
process-local and is never reconstructed after restart or transferred to a replacement session.
