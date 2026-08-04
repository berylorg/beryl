# CAS Phase 57 Publication Claim Fence Is Insufficient

## Invalid Approach

Serialize accepted-input publication and Ready-to-Delivering claims with an app mutex, then treat
the immediate route shape as the only proof that an ambiguous admission committed.

## Evidence

A scheduler worker may already have claimed an older input before admission begins. Its later
retry, completion, or rejection can advance the route during reconciliation. Provider loss, active
turn publication, terminal live events, and ordinary binding transitions can do the same without
passing through the scheduler claim path.

## Why It Fails

Fencing only new claims leaves many legitimate app publishers able to erase the immediate
one-step route shape. Extending one mutex to every current and future route publisher would couple
independent coordinators, create a high-omission cross-cut, and still make durable reconciliation
depend on process-local state. A whole-domain revision sandwich is insufficient too: unrelated
Syndic work can advance that revision indefinitely even though the immutable admission facts did
not change.

## Course Correction

The immutable accepted-input record retains the complete original admission intent. Exact
reconciliation compares that durable receipt plus permanent accepted-order and route-leaf identity
and accepts any later valid descendant. The exact path reads only immutable or identity-invariant
facts; the absent path performs one bounded retry around the source draft's natural identity
instead of waiting for global storage quiet. App callers use an opaque prepared admission and wake
the scheduler after exact reconciliation, but correctness requires no process-local publication
mutex.

## Phase 61 Recurrence

Cross-domain accepted-promotion reconciliation later repeated the same invalid pattern by
bracketing selective Syndic and asset-owner reads with the whole-home revision. Independent
completion review showed that an unrelated thread or domain could therefore force false
concurrency failures even when every promotion and relevant owner record remained unchanged.

The corrected cross-domain read brackets the selective Syndic classifier with two observations of
only the accepted-input and submitted-item asset heads. A change to either relevant owner requests
another reconciliation attempt; unrelated home commits do not.

## Affected Authority

This correction is part of Phases 57 and 61 in `doc/plan.md`,
`doc/systems/cas-live-syndic-transcript/design.md`, and `crates/beryl-app/doc/design.md`.
