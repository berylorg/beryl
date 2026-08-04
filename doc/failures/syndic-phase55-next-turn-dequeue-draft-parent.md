# Syndic Phase 55 Next-Turn Dequeue Draft Parent

## Scope

Ordered next-turn dequeue across Syndic thread, draft, route, and asset authority.

## Invalidated Approach

The Phase 55 summary treated dequeue as an atomic gate, new turn, accepted-route, and
accepted-input asset-owner transition followed by an app scheduler mount.

## Evidence

At discovery time, the Syndic conversation-history authority made a current draft's parent
immutable.
`AcceptedInputMutation` creates every replacement draft with the consumed draft's existing parent,
and reopen validation requires an ordinary current draft's parent to equal its thread's committed
tail. `IdleSubmissionMutation` later derives an ordinary submitted turn from that stored draft
parent.

After turn T accepts queued input A and leaves a possibly nonempty replacement draft parented to T,
dequeueing A must advance the thread tail to a new pending turn T2. A gate/turn/route-only
transition would leave the draft parented to T, so its later ordinary submission would branch from
stale history and reopen would reject the domain.

Current validation also forbids reusing an accepted input's raw identity as a submitted turn.
Dequeue therefore needs fresh turn and canonical-item identities plus durable route provenance
linking that terminal accepted input to its exact successor.

## Why It Failed

The planned boundary accounted for the queued input becoming an ordinary turn but not for the
surviving mutable draft that must follow the selected path. This is durable parentage authority,
not an app-only scheduling detail, and cannot be repaired by a compatibility mount or
process-local state.

## Course Correction

The Operator clarified that a draft is only unsent composer state. Queued input has already left the
draft, so later delivery must not rebase, rotate, clear, or otherwise mutate the current draft.

The accepted correction removes `DraftRecord.parent` entirely and replaces the two optional
special-purpose fields with one closed ordinary, discussion-context, or replacement submission
intent. Ordinary idle submission and accepted-input promotion derive a new turn's parent from the
transaction-current thread tail. Branch-first submission derives its exceptional parent from the
immutable context-envelope source, and replacement submission derives it from the validated target
turn.

Promotion uses fresh turn and item identities, retains the accepted input in permanent history with
an exact terminal successor witness, updates the current-draft reverse index only for the new thread
revision, and transfers accepted-input asset ownership to the submitted item. CAS delivery remains
a later monotonic step over that one pending turn.

## Affected Authority

The correction is reconciled in the composer and branch-discussion features, Syndic
conversation-history and CAS-live systems, image-asset authority, affected package designs, and the
root plan before source implementation resumes.

## Completion-Review Follow-Up

The first app correction removed thread revision from the storage draft update but retained it in
`DraftPersistenceBinding`, `DraftSaveToken`, and executor preflight. That manufactured a conflict
when the thread tail advanced before an otherwise exact same-draft save.

This remained the invalid draft-parent coupling at a higher layer. The accepted correction removes
thread revision from the app persistence binding and request as well. A tail advance that preserves
the current draft does not invalidate the save; only a genuine serialized conflict enters exact
draft-state reconciliation and retry.
