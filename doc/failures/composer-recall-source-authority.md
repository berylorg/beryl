# Composer Recall Source Authority

Ordinary accepted-input recall needs lasting proof of the exact acceptance source after submission
has disposed the editor and queued input may have been promoted. A proposed new acceptance-receipt
family would have changed Syndic's exact Home declaration and rejected existing homes. This was
identified during design review before source implementation and the new-family proposal was removed.

An ordinary disposal receipt plus equal content is also insufficient: session A may dispose cleanly
and session B may reopen the same published root and accept it. Combining A's receipt with B's input
would misattribute the accepting session. Existing first-acceptance outcome inspection closes this
with mutable fresh-draft and label-authority checks, which are unsuitable as lasting recall proof.

The [schema contract](../../crates/syndic-storage/doc/design-schema-v7.md#ordinary-recall-canonical-extension)
instead gives the original acceptance command a distinct acceptance-purpose form in the existing
session-disposal receipt namespace. It binds original request, source candidate/materialization,
disposal closure and exact acceptance result atomically. Ordinary disposal keeps its existing bytes
and grants no recall authority. The family declaration stays unchanged; process-local GUI history
does not require old-input backfill. Source lookup must authenticate the complete canonical receipt,
not synthesize acceptance from unrelated records or digest equality.

## Keep Repeated Build Evidence Compact

The first implementation embedded the full acceptance-purpose receipt in both the canonical build
header and the frontier. Even a short text recall emitted a 16,861-byte build value against the
existing 8,192-byte bound, so the original begin command correctly refused before changing the editor.
Repeating a bounded receipt does not establish that the enclosing record fits its own bound.

The corrected authority persists a compact receipt key/digest and exact thread/content/Asset
descriptors, authenticating the complete original immutable receipt at each authority-consuming
boundary. The complete receipt remains in its existing session family; no family or bound changes.
Owning tests must execute admission and terminal commands, in addition to checking codec validity.

Recall adoption also creates ordinary unpublished candidate provenance. The ordinary edit admission
path initially recognized only its previous committed-edit settlement form, so editing a recalled
draft returned an identity collision. Route recall/restore provenance through the shared candidate
authenticator and qualify the next ordinary edit, Undo and retention eviction after adoption; a
successful recall by itself does not establish continued editor usability.

The normal success finisher did not cover detached disposal: that path reconciled acceptance and
then discarded pending state directly. Share the exact acceptance-history capture operation at
both terminal paths, before discard. Qualify unresolved absence, exact original receipt after
disposal, and idempotence after stale advances; successor-widget tests do not cover this seam.
