# Syndic Phase 58 Current-Transcript Promotion Gate

## Invalidated Approach

Require `ProjectionLifecycle::Current` while constructing an accepted next-turn promotion
candidate.

## Evidence

Normal selected-turn finalization invalidates the transcript head to `Stale`. The Phase 58
prior-delivery regression could discover its queued input only after explicitly rebuilding that
projection, even though the idle gate, selected tail, route, binding, and draft reverse remained
exact.

## Why It Failed

Transcript projection lifecycle is rebuildable derivative state, not execution eligibility
authority. Treating `Stale` as unavailable can strand valid durable next-turn work behind an
unrelated presentation rebuild. Promotion already owns the atomic operation that supersedes an
active build and advances the exact transcript head to the new pending tail.

## Course Correction

Accept both exact `Current` and exact `Stale` transcript heads when their thread, committed tail,
and selected-path digest agree with the candidate basis. Preserve the head as an exact mutation
fence, supersede any active build, and publish the successor head as `Stale`.

The regression now promotes directly from the naturally stale post-finalization head and verifies
the prior delivery witness plus promotion reconciliation across reopen.
