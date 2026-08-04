# Syndic Phase 13 Route Generation From Input Order

## Scope

Phase 13 accepted-input route-generation identity allocation in `syndic-storage`.

## Invalidated Approach

The first route-generation cut derived generation identities from accepted-input order: queued
generations used even values and active steering epochs used odd values computed from the input
gate's accepted-input high-water mark.

## Evidence

The recovered-binding tests
`recovered_lineage_activation_requires_its_injection_process_and_preserves_chronology` and
`recovered_abandonment_retains_exact_active_snapshot_generation` execute two valid consecutive
turns without accepting an intervening fragment. The first activation persisted generation 1, the
accepted-input high-water remained zero, and the second activation recomputed generation 1 and
failed with `ActiveSteeringRouteConflict`.

## Why It Failed

Accepted-input order and execution-route epochs are independent monotonic domains. A turn can begin
without any accepted fragment, while an unselected next-turn generation can be created without
becoming the selected active route. Neither accepted-input high-water nor the selected route head
is therefore complete allocation authority.

## Course Correction

The input gate owns one compact monotonic route-generation high-water. Every newly created active
or unselected next-turn generation advances it atomically; revising or extending an existing
generation preserves it. The selected route proof remains selection authority only. Reopen proves
sequential allocation and exact agreement between stored generations and the gate high-water.

Focused proofs cover consecutive empty epochs, interleaved unselected generations, exhaustion,
constructor rejection, high-water drift, and generation gaps.
