# Syndic Phase 13 Pending Finalization

## Scope

Checkpoint 3 Phase 13 ordinary-turn preflight against Syndic admission and finalization authority.

## Invalidated Approach

The initial app preflight required a pending ordinary turn with one canonical user item to report a
finalized-item frontier of one before `turn/start` could execute.

## Evidence And Failure

`IdleSubmissionMutation` authoritatively creates that pending state with `item_count = 1` and
`finalized_item_count = 0`. Syndic finalization mutations intentionally require a proven-terminal
turn. Real end-to-end tests therefore rejected every newly submitted ordinary turn as unavailable
before any CAS request could be sent.

Treating the input as pre-finalized would also weaken the single ordered finalization frontier: the
same frontier must later advance across the admitted user input and captured response items only
after the provider terminal fact and required visible projections are current.

The later ProviderItemV1 fixture cutover exposed the same invalid assumption in an older active-turn
fixture. It retained one completed provider item inside `finalized_item_count = 1` while the turn
remained active. Exact provider replay correctly rejected the alternative states: a live provider
manifest could not satisfy that finalized frontier, while a finalized manifest could not belong to
an active turn. This was a stale fixture contract, not authority to weaken terminal-only
finalization.

## Course Correction

Ordinary preflight requires exactly one sealed canonical user-input item, no source events, pending
lifecycle, and finalized frontier zero. The input remains immutable and readable for CAS dispatch;
proven-terminal convergence later projects and finalizes it as ordinal one before advancing through
assistant and operational items.

No fixture-only state rewrite or premature finalization mutation is permitted.

Active-turn fixtures therefore retain completed provider evidence as live canonical capture with a
zero finalized-item frontier. They expose it through active capture state, not finalized transcript
entries; terminal convergence later performs the ordinary freeze and frontier advance.

## Affected Authority

- `doc/plan.md`, Phases 5, 7, and 13.
- `crates/syndic-storage/src/mutation/admission/idle.rs`.
- `crates/beryl-app/src/cas_projection/ordinary/preflight.rs`.
- Phase 13 ordinary execution integration tests.
