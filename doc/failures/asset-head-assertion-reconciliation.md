# Asset Head Assertion Reconciliation

Historical replacement selection requires the original command's exact outcome closure to bind
both the unchanged submitted-item head and the new draft head. Writer-admission validation alone
does not preserve that complete closure for acknowledgement-loss reconciliation.

On 2026-10-13, inspection of
`asset/mutation/owner_heads.rs::UpdateAssetOwnerHeads` invalidated reuse of its assertion arm for
this purpose. `reserve_reconciliation` counts only mutating updates, and `contribute` skips
assertions. The historical expectation would therefore be checked during admission but absent
from the persisted reconciliation descriptor. Ordinary marker-free validation-only participants
likewise cannot bind the two absent-head facts in this operation's descriptor.

The corrected [Asset contract](../../crates/beryl-state/doc/design-assets.md#asset-v3-and-reference-ownership)
uses one bounded selection participant. It contributes the byte-equal historical record without
changing its owner revision, alongside the new draft head. The marker-free branch contributes
absent-key deletes after authenticating both heads absent. HomeStore records both exact head facts;
the Asset domain revision advances normally. No new family, history mutation or set traversal is
needed. This contribution also requires the exact-unchanged reconciliation rule below.

Independent correction review found that the existing Asset classifier treats a matching record
with equal old/new values as ambiguous and therefore always reports collision. HomeStore also
requires every domain to select the same exact side, so another domain's changes cannot currently
discriminate a marker-free Asset contribution. The
[reconciliation contract](../../crates/beryl-home-store/doc/design-atomic-commands.md#reconciliation-and-operation-health)
now defines process-local exact-unchanged evidence. Such facts constrain both sides; ordinary
composition requires at least one changed domain to select old or new. All-unchanged composition
still collides, and first-acceptance successor eligibility remains unchanged. This bounded shared
prerequisite needs its own acceptance before replacement selection uses it.

Acceptance must verify unchanged historical bytes, atomic selection, both marker-free absence
facts, and exact reconciliation when either head disagrees. The general lesson is to inspect the
actual reconciliation contribution before reusing a validation assertion as original-outcome
evidence.
