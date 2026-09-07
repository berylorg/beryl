# Draft Marker Replay Cleanup Accounting

Terminal cleanup cannot count every persisted target leaf as one live marker association.
Assignment retains an old target leaf for exact receipt replay while replacing the live leaf;
the retained charge includes both leaves' bytes but only one logical association.

Fresh-asset cancellation tests exposed terminalization dropping the selected receipt without
reclaiming its shadow target leaves. Later cursor cleanup counted both physical leaves against
one association, rejected the charge subtraction, and could not finish cleanup. The failure
occurred after partial assignment and after readiness, independently of a stale proof consumer.

The correction authenticates the selected receipt's bounded replay predecessors and the current
target path, then deletes obsolete target leaves atomically with terminalization. It subtracts
their encoded bytes only; ordinary cursor cleanup retains the current tree's association charge.
Read, delete, and reconciliation reservations include the bounded predecessor work.

The public fresh-readiness tests now cover partial-assignment and ready cancellation, unchanged
head/receipt/capacity after a rejected cancelled-proof consumer, and complete cleanup. The focused
fresh, close, and tree run passed 27 tests. This preserves the existing
[Syndic admission contract](../systems/syndic-conversation-history/design.md); it does not introduce
a second cleanup policy. Future changes to replay retention must preserve the distinction between
physical encoded bytes and logical association cardinality.
