# Deferred Replacement Rebind

Historical import selects a new saved source and history baseline; it is not an ordinary widget
edit. The initial app contract assumed source rebind and imported-history installation could
always happen in the same GUI update.

Readiness review found that `RangeTextInput::rebind` may return `Busy` and retain a Direct intent.
`RangeTextInputElement::prepaint` can later service that intent, and
`commit_pending_direct_rebind` initializes the successor with unavailable history. The app cannot
assume its original control call installed the successor or supplied the imported frontier.

The corrected [adapter contract](../../crates/beryl-app/doc/design-catalog-and-composer.md#historical-replacement-adapter)
retains the exact admitted successor and interaction/publication gates. It observes logical
rebind completion, installs the authenticated imported frontier, and holds fresh-source
fulfillment/delivery until that installation. Only coherent successor realization publishes
replacement mode and transcript dimming. Busy does not justify recapturing the source, issuing
another import or fabricating an ordinary mutation.

Existing source-rebind APIs support this sequence. Preserved-resident prepublication adoption
cannot replace it: that separate boundary requires unchanged content, object identity/order,
history meaning and positions. Metadata-only replacement cancellation instead keeps the same
candidate source binding and history frontier, so it requires no widget rebind.

Native verification must exercise a genuinely deferred rebind and prove that successor input,
mode and dimming never publish with temporary unavailable history or lost baseline authority.
