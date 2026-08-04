# CAS Phase 70 Direct Hard-Run Election Gap

## Invalidated Approach

Releasing the primary stop-election permit as soon as the matching `turn/interrupt` response was
settled was initially treated as sufficient because the direct hard continuation still retained
the exact backend foreground binding and minted a fresh no-successor authorization before coarse
cleanup.

That boundary is invalid. The router election is the app-owned proof that terminal publication,
target loss, steering, and successor work cannot invalidate the selected parent between primary
settlement and hard-target authorization. Releasing it first lets terminal or target loss win that
gap while the backend binding remains locally present, after which the driver could authorize a
coarse thread-wide cleanup from stale app authority.

## Correction

When a hard slot is attached before primary settlement, the non-cloneable original stop-election
permit transfers from the primary dispatch owner into the hard-run continuation. The continuation
retains that permit through capability checks and fresh exact backend authorization, then releases
it before waiting for the hard-target response so ordinary terminal ingress remains concurrent.

Late hard attachment after confirmed soft-stop acceptance continues to acquire a fresh exact
router election on the same driver. Every no-target, unavailable, error, and drop path releases its
owned election exactly once while still publishing the frozen result and preserving ordinary stop
settlement semantics.
