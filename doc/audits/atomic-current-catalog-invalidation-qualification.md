# Atomic Current Catalog Invalidation Qualification

## Acceptance Boundary

Qualify the Home/State primitive needed to join an exact source mutation and named Catalog
invalidation in one serialized atomic command. This does not accept producer integration,
exhaustive coherent projection readiness, retained frozen reads or the Thread Switcher mount.

## Implemented Capability

- `CurrentHomeCommand` adopts one primary and distinct secondary sealed CurrentDomainCommands.
  It retains the owner and every participant cancellation signal through the ordinary admission
  handshake, preserves the primary typed fault scope and exposes no sidecar or validation-only
  route. The existing single-domain command contract is unchanged.
- `HomeStore::execute_current_home` captures physical Home/domain revisions after serialized writer
  admission and validates exact live registration and owner identities. It uses ordinary reservation,
  bounded preparation, batch publication, receipts, health classification and original reconciliation.
  Explicit candidate access applies its own admission gate to the same command machinery.
- `CatalogState::invalidate_current` seals one named-thread mutation. Writer preparation authenticates
  requested primary identity and complete primary/recency agreement, then advances the checked row
  revision and publishes both Stale copies. Facts, source revisions and recency remain unchanged.
  Already-Stale rows undergo the same authenticated advance; the older caller-fenced marker API
  retains its existing behavior. Missing, misbound, disagreeing or exhausted rows reject atomically.

## Verification Scope

Focused cases cover writer-time revision capture after unrelated commits, exact source rejection,
secondary preparation/contribution failure, empty contributions, duplicate and foreign participants,
all retained cancellation signals and cancellation after admission. Persistence cases preserve the
original two-domain receipt through ExactNew reconciliation and prove ExactOld after journal failure
and same-home generation recovery. Stale primary and secondary handles cannot publish a fresh effect.

State cases cover latest named-row facts, unchanged source/search/recency facts, Current and Stale
inputs, exact requested identity, missing/disagreeing copies, checked revision exhaustion, whole-home
validation, coupled source guards and original fault/outcome custody. Test-only record corruption and
revision seams are bounded to the exact pair and absent from ordinary production builds.

The existing Home/State regression packets cover shared typed ownership, bounded callbacks,
registration, admission, reentry/panic/drop guards, durability and recovery. No new dependency or
manifest change is part of this boundary.

## Accepted Qualification

Accepted on 2026-10-08 after independent Home/State integrity review cleared the complete frozen
17-path source set. Scoped rustfmt, diff checks and root/canonical SHA256 correspondence passed.

- Development Home/State all-target check passed. The complete initial regression packet ran
  546 cases: 543 passed and three new State fixture observations failed. Those assertions used
  ordinary access after Home failure or unwrapped an intentionally corrupt row. Test-only fixes
  preserve original outcomes and inspect fresh same-home candidate authority or the exact invariant
  refusal. The final complete Catalog target passed 33/33, run
  `9abaf077-6686-42a2-ad3a-cffea4b91c0f`, 2.584 seconds. Production source was unchanged by those fixes.
- Canonical HomeStore/State all-target check passed in 24.96 seconds, with exact final source bytes,
  no ignored local configuration and actual canonical compiler paths. Shared-target source switching
  refreshed the two package entry-point mtimes without changing their bytes.
- Complete canonical nextest passed 546/546 across 91 binaries, including all 12 new Home command
  and ten new Catalog invalidation cases. Run `e83ad0f8-303e-471d-9cee-65b1bec13da5`, 180.366 seconds.
  No baseline exclusion or skipped case was required.

Cargo used stable, the tracked LLVM linker, one build/test worker, debug information disabled and
incremental compilation disabled; Windows ErrorMode and process-local settings were restored.
Canonical qualification omitted ignored local dependency patches. No manifest/dependency changed.
Evidence was bounded to less than 1 MiB in `.tmp/atomic-catalog-invalidation-evidence`; no owned
Cargo process remains. The isolated Beryl checkout is retained for the next canonical boundary.

This accepts only the atomic capability. Producer completeness, compact-summary maintenance,
coherent rebuild readiness, frozen queries and visible mounting remain pending.
