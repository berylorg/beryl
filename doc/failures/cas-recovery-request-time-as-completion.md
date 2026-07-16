# CAS Recovery Request Time As Completion

## Scope

Checkpoint 3 Phase 10 recovered-injection establishment provenance.

## Invalidated Approach

The coordinator copied `CasProjectionRequest::observed_at` into
`RecoveredInjectionProof::completed_at` after CAS reported injection success.

## Evidence And Failure

The request timestamp is captured before remote projection work. An unclassified native failure
can then leave an explicit recovery decision in the composer for an arbitrary period before the
Operator chooses recovery. Reusing that original value as injection completion time can predate
the actual successful injection by minutes or longer.

The proof is durable ordering authority: later recovered-lineage activation must not predate its
recorded injection completion. A request-start observation cannot satisfy that meaning even though
the synchronous coordinator happens to dispatch activation later.

## Course Correction

The app coordinator observes Unix wall-clock time immediately after CAS reports exact injection
success and uses that local completion observation in the recovered proof. Clock conversion is
typed and fallible; it is never silently clamped, defaulted, or replaced with the request time.

The caller-supplied request timestamp remains available only for non-authorizing stale provenance,
where the system contract explicitly treats the timestamp as provenance rather than execution
authority.

## Affected Authority

- `doc/plan.md`, Phase 10.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 projection completion.
- `crates/beryl-app/src/cas_projection/execute/recovery.rs` and support/error boundaries.
- Recovered-lineage proof verification.
