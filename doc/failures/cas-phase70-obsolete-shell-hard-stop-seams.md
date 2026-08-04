# CAS Phase 70 Obsolete Shell Hard-Stop Seams

## Invalidated Approach

Deleting the old shell hard-stop modules and their direct tests while leaving imports, caller
methods, state projections, and rendering branches in other shell files was initially treated as
enough to establish the new CAS-owned boundary.

That boundary is invalid. Dormant references to removed shell-owned target and outcome types keep a
second architectural vocabulary alive and allow later GUI work to reconnect to an obsolete worker
instead of mounting the CAS projection service defined by authority.

## Correction

Phase 70 removes all obsolete shell hard-stop caller, state, and rendering seams without mounting
new GUI behavior. Its source-boundary regression forbids the removed module paths, target and
outcome types, and worker entry points so Phase 71 has only the authoritative CAS-owned integration
surface available.
