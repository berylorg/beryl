# Syndic Reopen Validation

## In-Bound Physical Corruption Cannot Reach Same-Home Recovery

Scope: Checkpoint 3 Phase 2 Syndic schema and reopen-validation verification.

The phase proposed proving ordinary reopen and same-home recovery rejection for every malformed physical and semantic Syndic record shape through test-only exact-domain seams.

`beryl-home-store` deliberately exposes only `HomeStore::inject_persisted_oversized_record` under `test-faults`. The seam requires an exact typed domain handle and codec owner and accepts only a record that exceeds the codec's stored key or value envelope. `MutationBuilder` always writes the exact codec-owned record version and encoded key/value, so a registered Syndic domain cannot use it to persist an unsupported version prefix, malformed in-bound payload, invalid stored sentinel, or other arbitrary physical envelope.

An alternate exact-name shadow domain can seed those shapes before ordinary reopen, and typed Syndic test mutations can create semantic contradictions after registration. Neither mechanism can place every in-bound physical corruption into the already registered same-home generation for an exact recovery test.

Widening the home-store fault seam would change the accepted package contract in `crates/beryl-home-store/doc/design.md`, which currently defines its sole persisted-corruption seam as oversized-record-only. Quietly adding raw storage access or weakening the Phase 2 matrix would contradict architectural authority.

Course correction: stop Phase 2 before schema implementation. The Operator must explicitly choose either a bounded test-only physical-envelope corruption extension owned by `beryl-home-store`, with corresponding authoritative documentation and tests, or narrower Phase 2 wording that assigns in-bound physical corruption to ordinary reopen while retaining semantic and oversized-record same-home recovery proofs.

Affected authority: `doc/plan.md`, `doc/rework/beryl-home/REWORK.md`, `crates/beryl-home-store/doc/design.md`, and `crates/syndic-storage/doc/design.md`.

Resolution: the Operator authorized the bounded `test-faults`-only extension. The home-store seam must require an exact current typed domain handle and codec owner, accept only a physical envelope rejected by that codec, enforce fixed fixture-byte ceilings and `SyncAll`, and expose no production or reusable raw-storage API. Phase 2 may proceed under that constraint.
