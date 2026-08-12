# Theme Service Copy Custody

## Scope

Phase 107 theme-operation reconciliation custody and fresh-service identity.

## Invalidated Approach

Preserve `BerylState: Copy` through a copyable theme-service binding that materializes the retained
operation registry when `BerylState::themes()` is called.

## Evidence And Failure

Implementation audit showed that separate `themes()` calls created distinct `Arc` registries.
Handles for the same state generation therefore did not share indeterminate-operation custody, so
dropping one handle or issuing work through another could bypass exact-scope gating.

## Correction

`BerylState` owns one non-`Copy` `ThemeService` registry for its generation and exposes clone-based
access. Service clones share the registry; fresh acquire or same-home reacquire creates a fresh
registry; the last state, service, guard, and subscription drop releases it. App consumers must
borrow or clone `BerylState` explicitly.

## Affected Authority And Verification

The correction implements Phase 107 of `doc/plan.md` and the custody, freshness, and bounded-release
requirements in `doc/systems/theme-runtime/design.md`. Focused tests must prove shared clone gating,
fresh-generation separation, and final registry release.
