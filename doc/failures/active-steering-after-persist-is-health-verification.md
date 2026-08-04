# Scope

Active-steering claim and checked-user terminal publication under Beryl-home persistence faults
that move the home to `verifying`.

# Invalidated Approach

The test `after_persist_claim_fault_reconciles_exact_before_dispatch` expected an injected
post-persistence error to reconcile the durable claim and continue to backend steering dispatch.
The checked-user test `after_persist_terminal_reconciliation_proves_one_event_and_binding_transition`
likewise expected same-operation storage reads and terminal publication after the same fault.
The older `definitive_terminal_publication_failure_closes_without_a_terminal_event` test repeated
that assumption after a scoped `BeforeCommit` failure.

# Decisive Evidence

The focused `test-faults` run fails deterministically with `Loss(StorageUnavailable)`. The
`AfterPersist` fault is classified as a persistence verification failure and moves home health from
`Healthy` to `Verifying`. Active-steering reconciliation can no longer prove the same healthy
generation, and its subsequent loss publication also rejects storage while verification is active.
The checked-user path independently fails its first post-fault storage read with a `Verifying`
health-gate error before it can prove an event, binding transition, or terminal target.
The scoped `BeforeCommit` path reaches the same health boundary and therefore cannot use gated
reads to prove durable absence in the failing operation.

# Why It Failed

Persisted bytes do not by themselves preserve command authority after an ambiguous persistence
failure. The health gate becomes the sole verifier before more backend or durable work may proceed.
Continuing steering dispatch would bypass that authority even if the claim can be observed in
storage.

# Course Correction

An `AfterPersist` claim fault must fail closed into the existing verification/recovery boundary and
must not dispatch steering or continue checked-user terminal publication. The stale tests should
assert that behavior without gated storage reads. `verifying` is a nonterminal health pause: it does
not itself elect the persistent-failure cut or close the live-command gate, and the terminal test
must preserve that distinction while proving that no terminal outcome was published. If a future
test needs an ambiguous result that remains healthy and may reconcile into dispatch or publication,
it requires a separate explicitly designed fault contract; `AfterPersist` cannot stand in for it.

# Affected Work

The complete Phase 85 `beryl-app --features test-faults` gate exposed this pre-existing expectation,
but the Phase 85 lifecycle admission seam is not causal. Changing active-steering reconciliation or
adding a new cross-crate fault contract is separate architectural work.
