# Scope

Phase 70 process-local hard-stop activity tracking for an already active CAS target.

# Invalidated Approach

Derive the exact Syndic turn for every published provider activity from the source-publication
permit's pending turn activation.

# Why It Failed

`TargetTurnRegistration::Active` has no pending activation. The source-publication permit therefore
cannot recover a Syndic turn from activation authority for an already active target. Treating that
field as universally present would panic or force an inexact fallback precisely on the live path
that hard-stop attachment must authenticate.

# Course Correction

Derive the Syndic thread and turn from the durable `LiveSourceTarget` resolved by the provider
consumer after exact route admission. Combine that durable identity with the permit's loaded
generation and CAS thread/turn. Apply the compact activity effect only after durable publication
and successful permit completion.

# Affected Authority

This correction preserves Phase 70 of `doc/plan.md` and the exact snapshot contract in
`doc/systems/cas-live-syndic-transcript/design.md`. It requires no target-state design change and no
router registration expansion.
