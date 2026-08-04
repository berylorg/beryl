# Phase 72 Provider Stop Read Authority

## Scope

Context-compaction stop admission, live authentication, and restart convergence.

## Invalidated Approach

The first mounted implementation reused the ordinary active-turn delivery-recovery classifier to
authenticate every `Stopping` gate after stop admission and during restart.

## Evidence

- Ordinary stopping recovery requires the blocked turn to be the current conversation tail, a
  selected `NextTurn(Stop)` route, and an `Active` binding.
- Context compaction instead owns a parentless provider-operation turn, preserves
  `NextTurn(Compaction)` without a stopped steering route, and retains a `Valid` binding.
- The provider-aware stop observation already authenticates the compaction record, published CAS
  turn, snapshot, valid binding, and paired stop record, but the generic read classified the gate
  before that authority could be returned.

## Why It Failed

Ordinary turn interruption and provider-operation interruption share a stop record and backend
request, but they do not share durable route, binding, or recovery invariants. Treating the common
request shape as common read authority made a valid provider stop appear corrupt immediately after
its atomic admission.

## Course Correction

- Keep the ordinary active-turn classifier unchanged.
- Classify a provider-operation `Stopping` pair through dedicated exact compaction/stop authority.
- Represent the absence of an ordinary stopped route explicitly; never fabricate one.
- Prove immediate live authentication and restart abandonment without request replay.

## Completion Review Follow-up

The dedicated classifier authenticated the stop nonce and provider-operation state but the stop
admission witness retained only gate revisions. It did not retain the immediate source and successor
compaction revisions produced by the atomic handoff, so an impossible `Stopping` operation revision
could still satisfy live and reopen authentication. Provider-stop authority must retain and
cross-check that exact compaction transition through abandonment and its settlement receipt.

## Fresh Completion Review Follow-up

The corrected admission witness and live `Stopping` classifier proved the exact handoff and its
ordered provider descendants, but that proof stopped when the stop record was consumed. Safe reopen
retained only a loose successor floor, matching terminal retained no compaction revision cut, and
abandonment accepted any receipt source at or after the admission successor. Each provider-specific
stop successor must retain its own exact source and immediate successor compaction revisions and
carry the authenticated stopping ancestry through public reads, reopen validation, terminal
finalization, and the abandonment settlement receipt.

## Affected Authority

- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/plan.md` Phase 72

## Resolution

Provider-operation stops now use dedicated authority from admission through every live and consumed
successor. Admission, `Stopping`, safe reopen, matching terminal, and abandonment retain and validate
the exact source `Stopping` revision and immediate successor compaction revision; shared frontier
algebra admits only witnessed later provider or request descendants, and abandonment authenticates
the exact settlement receipt. Corruption coverage rejects lower, unwitnessed-higher, and coherently
shifted safe-reopen, terminal, and abandonment provenance.
