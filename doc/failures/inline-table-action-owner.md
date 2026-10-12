# Inline Table Action Reservation

## Failure

On 2026-10-12, the first native inline Table qualification reached an indexed-only owner
assumption in `refresh_table_source_reservation`. Rendering refreshes the reservation before any
Copy or Save command, so expecting an indexed resource owner caused every inline fixture to panic.
The native failure path then tried to retire an already closed window, producing a second panic
across the native dispatch boundary.

## Correction

Reservation refresh must dispatch through the actual Table source owner, including inline
projection ownership. Direct inline actions do not acquire an indexed resource identity. Native
fixture retirement checks that its window remains available before attempting teardown, preserving
the original failure for propagation outside the application event loop.

## Reuse

Review shared Table helpers invoked during rendering as well as explicit action entry points.
Successful provider and pure selection tests do not qualify mounted reservation callbacks. Genuine
inline native fixtures are necessary to exercise this path. Runtime acceptance remains in the
root implementation plan and mounting audit.
