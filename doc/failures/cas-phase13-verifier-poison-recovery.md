# Scope

Phase 13 request-scoped streamed `UserMessage` echo verification.

# Invalidated Approach

The first verifier implementation recovered poisoned verifier and verifier-slot mutexes by taking
their inner values and continuing correlation.

# Evidence And Failure

A panic while installing or advancing the verifier can leave its lifecycle frontier, source replay,
or installed-scope state only partially updated. Recovering the poisoned value would let later
notification bytes or a successful `turn/start` response be accepted under uncertain proof state.
That contradicts the fail-closed exact-correlation contract and could publish a request as matched
without proving both complete echoes.

# Required Course Correction

- Treat verifier or slot poison as typed `VerifierUnavailable` correlation failure.
- If poison is observed before transport bytes, classify the start as proven not dispatched and
  leave the session reusable.
- If poison is observed after dispatch or during ingress, fail the exact request and retire its
  connection authority.
- Never recover, reset, or replace the verifier inside the same request.

# Affected Authority And Proof

The correction affects only backend request-scoped verifier ownership. Focused tests must prove
zero source reads and reusable authority for install-time poison, plus typed fail-closed handling for
poison observed during correlation.
