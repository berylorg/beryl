# Syndic Storage Stale Target Graph

## Invalidated Assumption

Removing the whole-payload submission and replacement APIs was sufficient to complete their cutover.

## Evidence

Parallel locked all-target output exposed only a nondeterministic subset of failing targets. A
metadata-driven per-target check established 56 failing integration targets of 82 and one failing
example of six. Focused suites had missed that each Cargo integration target recompiles shared
support, so historical target roots and fixtures still named the removed APIs.

## Accepted Correction

Delete obsolete-only targets and the obsolete example, replace shared fixtures with canonical empty
`DraftRootHistoryPairV1` creation and fault-only record injection, and make each fault-only root
explicit. Rewrite the remaining live targets directly against target-state APIs in Phases 157-160.
Never restore the removed APIs or add compatibility mutations.

## Scope And Remaining Risk

Phase 156 removes the stale target-graph residues and repairs its three owned feature gates. The
remaining live target failures are deferred only to Phases 157-160 and require their own direct
target-state rewrites. This is distinct from the composer-successor boundary lesson in
`syndic-streamed-composer-successor-boundary.md`.
