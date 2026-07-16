# Scope

Phase 13 ordinary-turn publication and convergence across concurrent Syndic threads.

# Invalidated approach

The app sampled the home and Syndic domain revisions before entering
`HomeStore::execute`, built a revision-checked command, and reconciled the result after the writer
returned. Multi-record orchestration snapshots likewise retried whenever the whole Syndic domain
revision changed.

# Evidence

`HomeStore::execute` acquires the process-wide writer only after the caller has built its command.
An unrelated thread can therefore commit between the app's revision reads and writer admission.
The resulting ordinary command reports a revision conflict even though every request-owned record
still has the exact expected revision. Live-event capture treated that conflict as execution loss,
and its cleanup used the same racy path. Whole-domain stable-read loops could also retry forever
while unrelated threads continued committing.

# Why it failed

Home/domain revisions serialize physical commands, but they do not express logical ownership of one
thread. Sampling them outside writer admission coupled independent threads and violated the accepted
cross-thread concurrency contract. Blind retry would remain starvation-prone and could not make a
cleanup command authoritative.

# Course correction

Add a permanent typed home-store boundary that captures current physical revisions only after
serialized writer admission while preserving each domain mutation's exact record-level validation.
Use it for Phase 13 publication and convergence. Stabilize read-only orchestration snapshots by
re-reading their exact mutable anchor records, never by waiting for the entire domain to become
quiet. Add adversarial tests with sustained unrelated-thread commits.

# Affected authority

`doc/plan.md` Phase 13, `doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/beryl-home-storage/design.md`, and the owning package design documents must describe
the corrected boundary. Phase 13 remains in progress until the concurrency proofs pass.
