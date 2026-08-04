# Invalidated Approach

Phase 54 initially made a requested target close consumer-visible as soon as the router recorded
it, even when an exact provider-publication or active-steering permit still owned the route.

# Why It Failed

The close request and the close publication are different state transitions. Publishing the
terminal signal and dropping the sender while an exact permit remained active let the consumer
observe closure before the owner had either finished its publication or transferred atomically to
durable target-loss authority. It also contradicted the router's existing publication-permit
regression contract.

# Architectural Correction

An in-flight permit now records only the deferred close reason. The active-steering owner observes
that reason through its private attempt-status channel, while ordinary consumer polling remains
quiet. Finishing the exact owner publishes the deferred close, and the loss path may instead
replace the attempt atomically with durable loss-publication authority.

# Reusable Lesson

Internal cancellation visibility must not be implemented by prematurely publishing an externally
terminal state. A non-cloneable authority remains the linearization fence until it finishes or is
atomically replaced by the next authority.
