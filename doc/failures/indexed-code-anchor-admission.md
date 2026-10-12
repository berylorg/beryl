# Code Anchor Capture Before Range Admission

Retaining the last admitted source anchor is insufficient when a user scrolls outside its descriptor
range and a revision arrives before the next range response. The old anchor still names a different
line. Preserving its original fractional offset avoids a fabricated position but remapping that
anchor can move the viewport back to the previously admitted line.

`CodeViewport::rebind` and the regression
`code_rebind_fetches_the_current_line_when_revision_precedes_range_admission` demonstrate the case:
the last resolved line is 75, scrolling reaches line 76 with a 12.25-pixel remainder, and the
successor must preserve line 76 rather than resolve the old line-75 anchor. Indexed delivery admits
only current exact owners; fetching an obsolete owner's missing descriptor after replacement is
not an available correction.

The correction captures a matching resident descriptor when available. Otherwise it clears the
stale anchor and requests the current bounded line range from the successor. Storage's
`ProjectionTextSource::can_extend` permits only an unchanged Composer source or an authenticated
append-only ProviderNarrative prefix. For a compatible source, the successor descriptor supplies
the current line's exact source position; incompatible sources and changed block starts retain
their existing origin reset. No source scan, historical-owner exception or additional text cache
is required.

The [Code panel contract](../gui/widgets/code-panel/spec.md) and
[indexed structure authority](../systems/syndic-conversation-history/design.md#indexed-text-resource-structure-queries)
remain controlling. Supplied-response qualification does not establish actual provider or native
revision-transition acceptance, which remains in the root plan.
