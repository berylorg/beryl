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
