# Composer Terminal Marker Range

## Invalidated Boundary

Beryl forwarded the text-input's zero-width-object interval directly into Syndic's half-open
marker range. The two types used identical numeric endpoints but had different edge ownership: the
widget interval includes objects at both byte edges, while the Syndic range excludes the terminal
edge.

## Decisive Evidence

After one marker was inserted at exact source end, the mounted surface retained a caret position
after that marker while both delivered object pages claimed a complete empty envelope. A second
ordinary marker insertion therefore failed closed with `InvalidObjectGapProof`. Once the terminal
marker was returned, the same regression exposed a separate durable-splice defect: insertion at
`AfterAll` selected the preceding text edge when every existing same-anchor marker sorted before the
new marker, producing order 2 before order 1 and a `TreeLimit` rejection from canonical child-order
validation.

## Rejected Corrections

Adding one to the terminal byte would overflow or exceed exact EOF and would change byte semantics.
Merging a half-open range read with an exact-anchor read would create split cursor, edge-proof,
ceiling, lifecycle, and cancellation ownership for what is externally one page. Weakening the
widget's gap proof would admit a selection that the delivered object facts do not prove.

## Corrected Boundary

Syndic owns a distinct cursor-paged inclusive marker scope alongside its unchanged half-open scope.
Beryl maps the widget interval to that one request. Durable marker insertion separately derives the
ordered same-anchor insertion boundary: when no greater marker exists, it selects the boundary after
the authenticated run rather than falling back to the text edge before the run. Both paths remain
logarithmic or page-bounded and retain no complete marker collection.
