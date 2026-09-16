# Terminal Repair Dispatch Must Be Durably Once

## Scope

The private exact terminal-turn repair request used after live capture becomes repair-required.

## Invalidated Assumption

A process-local non-cloneable capability plus a same-thread successor gate is sufficient to prove
that the backend repair request is dispatched at most once.

## Evidence

The process can fail after the backend may have accepted the request but before Beryl records its
result. On restart, the durable repair-required gate proves that repair remains unsettled, but a
process-local capability proves nothing about the earlier dispatch. Reconstructing a fresh
capability could therefore issue a second request whose backend effects cannot be deduplicated by
Beryl.

## Why It Fails

The repair adapter is intentionally non-idempotent and admits no reread, cursor traversal,
adjacent-turn, item-history, or whole-thread fallback. A possible prior dispatch must therefore be a
durable terminal fact, not an invitation to retry.

## Course Correction

Syndic owns one durable target-scoped repair-request claim. The app atomically consumes it before
backend dispatch and derives the only private backend capability from that consumed disposition.
The backend requires that capability. A consumed but unsettled claim survives process loss as
explicit-incomplete authority and can never authorize a second request. Both repaired and
explicit-incomplete dispositions still pass through `FinalizingHistory` before gate release.

## Remaining Risk

Implementation must prove every crash cut around claim consumption, request dispatch, backend
refusal, response staging, atomic snapshot selection, and finalization. No recovery path may
reconstruct or infer an unused claim after possible dispatch.

## Post-Gate Custody Gap

On 2026-09-17, phase 468 integration exposed a missing persistence decision. The
[Syndic schema](../../crates/syndic-storage/doc/design-schema-v7.md) embeds the request disposition
in `RepairRequired`, while incomplete convergence must replace that variant with
`FinalizingHistory(target)`. Its staged snapshot head carries consumed provenance only for staged
responses; it is not a universal outcome for unavailable repair or response loss.

The current `TurnStateRecord` retains terminal status and a closed incomplete reason, but neither
the request disposition nor resolved repair provenance. Authenticating the same retained terminal
source against a fresh gate revision therefore cannot distinguish initial repair entry from entry
after an incomplete resolution. Advancing a revision alone does not preserve a never-reset fact.
This is an architecture-readiness gap, not an observed duplicate backend request.

Independent review confirmed the gap. The unfinished gate integration was removed; accepted
witness, codec and retained-target components remain intact. No repair entry or dispatch API was
published by this attempt.

The Operator approved the correction on 2026-09-17: a bounded optional resolved-repair
extension in the existing turn-state authority. It retains the exact original target and capture-gap
witnesses, the closed resolution and original `Available` or `Consumed` disposition, including
the consumed attempt nonce and claim revisions. Gate exit installs it atomically with
`FinalizingHistory`; later gate release and turn-state updates preserve it. Initial repair admission
rejects a turn with resolved repair authority, even if its original disposition remained `Available`
because the adapter was unavailable. No snapshot or asset is invented for incomplete resolution.

The owning schema now specifies the V4 turn-state representation, revision/codec rules,
retained-evidence validation and replay behavior. Verification must prove no re-entry
after resolution, reopen or acknowledgement loss, exact disposition preservation, and atomic
outcome/gate publication. Implement the turn-state component before phases 468 and 469; this does not authorize a new
backend route or weaken the at-most-once contract.
