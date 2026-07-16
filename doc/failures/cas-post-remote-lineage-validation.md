# CAS Post-Remote Lineage Validation

## Scope

Checkpoint 3 Phase 10 fresh, resumed, forked, and recovered CAS projection establishment.

## Invalidated Approach

Native lineage constructors were invoked after CAS had already started, resumed, or forked a
thread. Recovered-injection proof construction likewise used `?` after exact injection success.

## Evidence And Failure

Native proof validity depends only on the already prepared source and target basis, so validating
it after remote dispatch creates an unnecessary failure cut. If a constructor rejected, the fresh
remote identity could leave scope without the coordinator's typed stale-provenance choreography.

The recovered proof additionally requires the loaded generation and completion timestamp, which
exist only after registration and injection. Its fallible constructor therefore cannot move wholly
before dispatch, but failure must still consume the loaded target through explicit abandonment.

## Course Correction

Construct native fresh, resume, and fork lineage proofs before their first remote request. After a
successful recovery injection, classify both completion-time observation failure and recovered
proof-construction failure through the same explicit target-abandonment path, retaining exact
known prefix, profile, zero native count, and loaded-generation provenance without inventing a
usable lineage proof.

## Affected Authority

- `doc/plan.md`, Phase 10.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 projection completion.
- `crates/beryl-app/src/cas_projection/execute/fresh.rs`, `native.rs`, and `recovery.rs`.
- Remote/local cut-point verification.
