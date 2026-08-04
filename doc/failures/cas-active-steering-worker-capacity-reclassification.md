# CAS Active-Steering Worker-Capacity Reclassification

## Invalid Approach

Treat local worker-pool exhaustion or an occupied connection steering attempt as a durable
`NextTurn(WorkerCapacity)` disposition for an otherwise steerable accepted input.

## Evidence

The active-steering operation acquires bounded local execution capacity before dispatch. Exhaustion
therefore proves only that Beryl cannot run another delivery attempt at that instant; it says
nothing about whether the exact CAS target accepts steering. Durable accepted-input storage already
retains ready work without a resident task or payload.

## Why It Fails

The transition changes user-visible conversation structure for an implementation-local scheduling
condition. A fragment intended for an active steerable turn can become a later ordinary turn even
though neither CAS nor the target lifecycle made steering ineligible.

## Course Correction

Keep fixed worker and connection-attempt bounds, but make saturation a transient no-op on durable
route state. Leave the input ready or retryable for the same exact target and let a later bounded
scheduler pass try again. Only authoritative non-steerability, structured rejection, or projection
loss may reclassify accepted steering input as next-turn work.

## Remaining Risk

The mounted scheduler must wake or make another bounded pass after capacity is released without
retaining one process-local waiter per accepted input.
