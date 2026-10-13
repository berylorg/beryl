# Draft History Head Accounting

An empty transition history still has a charged frontier record. A sealed-import baseline cannot
use zero retained encoded bytes, and an immutable baseline and mutable Session fork cannot differ
only in key and digest while claiming equal retained totals.

The 2026-10-13 replacement-import authority draft made the latter assumption. Inspection of
`draft_piece/history/append.rs::stored_frontier_charge` showed that charge includes the complete
canonical family key and value, including the repeated key. `history/records.rs::fork_session`
and `publication_snapshot` explicitly subtract the source head charge and add the destination
head charge. A Session key is 33 bytes; the new baseline key is 49 bytes. Their encoded values
also repeat those different keys, so equal totals would invalidate retention authentication and
the first real history append.

The corrected [schema](../../crates/syndic-storage/doc/design-schema-v7.md#sealed-import-canonical-extension)
keeps transition charges and history links empty while charging each actual frontier head. It
uses existing snapshot/session conversion, requires both heads to fit the retained budget, and
binds their exact accounting difference in baseline/fork provenance. It introduces no fake edit,
relaxed accounting or later history reset.

Runtime qualification must cover the exact budget threshold, baseline/fork correspondence and
the first ordinary edit and Undo. Those checks remain part of durable replacement-draft acceptance.
