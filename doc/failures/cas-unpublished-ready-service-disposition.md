# Unpublished Ready Service Disposition

## Scope

CAS same-home service recovery and post-adoption publication failure cleanup.

## Invalidated Assumption

`ProjectionConnectionService::dispose_unpublished_inert` asserted that every unpublished service
still had dormant startup state. That was true before recovered startup convergence became a
required pre-publication phase, but it is not an invariant of the final architecture.

## Evidence

Phase 84 final-publication tests reached four valid pre-commit owning-error paths after recovered
startup had converged. Each path panicked at the dormant-only assertion in
`crates/beryl-app/src/cas_projection/service.rs`, even though the shared startup publication gate
was still closed and the existing cleanup path could cancel and join the ready workers safely.

## Correction

Unpublished inert disposition accepts either dormant startup or converged-ready startup. Execution
authority is determined by the still-unpublished startup gate, not by treating `Ready` as proof of
process publication. Cleanup cancels the gate, joins all reached workers, shuts down the epoch
provider, and preserves the supervisor-retained home.

## Affected Authority And Coverage

This correction implements root `doc/plan.md` Phase 84 and the same-home recovery contracts in
`doc/systems/cas-live-syndic-transcript/design.md` and `crates/beryl-app/doc/design.md`. Focused
publication-order, slot-rejection, and partial-worker-arm tests cover the corrected disposition.

## Remaining Risk

Future startup states must define their unpublished inert disposition explicitly before they are
allowed between convergence and process publication.
