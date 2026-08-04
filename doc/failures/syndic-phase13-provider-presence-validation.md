# Scope

Phase 13 item 6 destination-owned unpublished provider-observation validation.

# Invalidated Approach

Treat the observation-kind field allowlist and required-field presence bitmap as the complete
destination grammar. Once a field was allowed for that item or delta, mark it present for any
scalar, enum, text, list, or structured control using that field context.

# Why It Failed

Field presence is not typed validation. That approach can seal an agent message whose timestamp,
item identity, and narrative are all `Null`; accept an enum from an unrelated status vocabulary;
substitute a scalar for a text or container value; or admit duplicate and conflicting field
representations. The backend is an upstream validator, but durable unpublished Syndic state is the
publication input and must remain trustworthy after restart, fault reconciliation, or a future
producer defect.

Checking only required fields would make item 8 publish malformed typed state or reimplement the
same grammar during publication. Either outcome destroys the intended single resumable destination
validation boundary.

# Course Correction

The Syndic validator owns an exact field-to-value grammar for every field admitted into each
normalized observation kind. Its fixed resumable state enforces scalar type, enum domain, text
lifecycle, container and element shape, structured entry context and depth, bounded
`CasItemId`/`CasThreadId` text semantics, duplicate rejection, and completion rules. Treating those
identities as generic text is the same class of destination-grammar loss even when the upstream
decoder already validated them. Wire-only fields deliberately consumed and discarded by the
backend privacy or resource boundary are not normalized fields and cannot be required or recreated
by Syndic; see `doc/failures/syndic-phase13-private-reasoning-normalization.md`. The sole lossy
Web-search `Other` marker remains accepted only in its declared action field and still records
monotonic unsupported-history evidence.

Backend validation remains necessary for protocol ownership, early rejection, privacy exclusions,
and safe streaming. It is not a substitute for destination validation, and item 8 may trust a
sealed observation only because Syndic independently proved the typed grammar.

# Verification Consequence

Each positive item and delta schema requires paired negative coverage for wrong scalar and enum
types, text/scalar/container substitution, duplicate fields, malformed nesting and indices,
cross-field enum reuse, wrong `Other` placement, restart persistence, and exact rejection without
published state.
