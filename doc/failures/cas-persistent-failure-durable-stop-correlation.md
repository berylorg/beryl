# Persistent-Failure Interruption Is Not A Durable Stop Correlation

## Scope

Checkpoint 3 running-session Beryl-home recovery and its predecessor exact-interruption cut.

## Invalidated Approach

Reuse `StopOperationCorrelation`, `StopAttemptCorrelation`, and the durable
`ExactForegroundTurnAuthorization` to send the one volatile best-effort interruption allowed after
persistent storage failure, while relying on app comments to say that no durable stop existed.

## Why It Failed

The backend types explicitly bind their correlations to a durable stop operation and its sole
claimed attempt. Persistent failure prohibits new durable stop admission after the store gate has
closed. Fabricating those values would therefore make the Rust capability claim authority that was
never admitted, even though the provider wire contains only the exact thread and turn ids.

Generalizing the existing names would also weaken the durable stop boundary and allow future code
to interchange two request causes with different lifecycle, retry, and persistence rules.

## Required Course Correction

- Keep the durable stop authorization and correlations unchanged and stop-specific.
- Add a separately typed process-local persistent-failure correlation, exact authorization,
  request, and outcome family in `beryl-backend`.
- Reuse only the internal exact-target validation, authorization epoch, no-successor fence, pinned
  wire dispatch, and normalized response classifier.
- Keep target selection, failure-generation guarding, prior-dispatch evidence, and all durable
  state outside the backend package.
- Prove at the type boundary that neither authorization family can be passed to the other's
  dispatch method.

## Affected Authority

- `doc/plan.md` Phase 76 and its persistent-failure successor
- `doc/systems/backend-runtime/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-backend/src/hard_stop.rs`
- `crates/beryl-backend/src/session/interruption.rs`
