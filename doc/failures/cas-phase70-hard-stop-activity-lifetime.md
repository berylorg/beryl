# CAS Phase 70 Hard-Stop Activity Lifetime

## Invalidated Approach

Clearing process-local hard-stop activity only when terminal or authority-loss handling found a
local stop operation was treated as sufficient because the activity was introduced for stop
snapshots.

That is invalid. Provider activity exists before any stop is admitted. An ordinary terminal or
source loss with no local stop could therefore leave the completed turn's activity entry resident
indefinitely, violating the live-target residency bound and allowing stale state to survive until a
later operation on the same Syndic thread.

## Correction

Every exact provider terminal and target-authority-loss path supplies both Syndic thread and turn
identity to the stop coordinator. The coordinator clears only a matching activity entry before it
branches on local stop ownership. Stop-slot consumption remains conditional on the matching local
operation, while activity lifetime follows the exact live target independently.
