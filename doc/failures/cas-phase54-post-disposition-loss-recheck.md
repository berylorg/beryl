# CAS Phase 54 Post-Disposition Loss Recheck

## Invalidated Approach

After an exact success, retry, or structured-rejection transition became durable, recheck the
mutable live-target status before releasing the checked lifecycle and steering-attempt owners. If
loss had meanwhile been requested, try to replace the attempt with target-loss authority.

## Evidence

A terminal source-publication permit can coexist with the steering attempt. Once exact success or
structured rejection removes the last live steering route, that permit may publish a proven
terminal turn while an ordinary target-loss request waits behind both owners.

Exact retry is different: it preserves the live steering count. Syndic's terminal gate rejects
terminal publication until that retained steering work reaches a removing disposition. A retry
racing ordinary loss must therefore commit Retry first and then converge incomplete projection
loss; it cannot coexist durably with a proven terminal.

The later status recheck classified the pending loss request as closure without representing the
stronger proven-terminal state. Loss transfer then waited for the source permit and rejected its
terminal result. Cleanup of the errored steering permit could subsequently replace the proven
terminal signal with a generic publication failure and strand the ordinary loss waiter.

## Why It Failed

The exact durable disposition was already the winning linearization event. Consulting later
mutable router state attempted to choose its authority a second time and turned an ordered exact
disposition followed by proven terminal capture into a false loss-transfer conflict.

Permit cleanup also treated every unfinished steering owner as stronger than an exact terminal
source owner, even though cleanup carries no provider evidence.

## Architectural Correction

Once success, retry, or structured rejection commits exactly, the delivery owner releases its
checked lifecycle and finishes the steering attempt without a second target-loss election.
Ordinary loss remains fenced until that atomic finish. If the finish reports deferred loss, the
delivery owner uses the ordinary loss path to reconcile against the current durable binding.
Where the committed disposition removed live steering, an independently proven terminal source
outcome is preserved instead of being replaced. Where Retry retained live steering, ordinary loss
converges the binding and reclassifies the delivery as projection lost.

Only delivery paths that have not committed an exact disposition may atomically transfer the
steering attempt into generic or named loss authority. Finishing or abandoning a steering permit
must preserve an already proven terminal target for its normal handoff and for waiting loss
convergence.

## Affected Work

`doc/plan.md` Phase 54, exact active-steering settlement, steering-permit cleanup, the router's
terminal-versus-loss race coverage, and the app and CAS-live/Syndic design descriptions own this
correction.
