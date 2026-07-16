# Scope

Phase 13 app capture of the submitted `UserMessage` lifecycle after CAS correlates it with the
already durable Syndic user item.

# Invalidated Approach

The first app capture path validated every started provider item through the generic
`capture_item` composite read used for provider-created text and resource items.

# Evidence

The cross-thread quiet-capture fixture converged the first turn to `Incomplete`. Isolating the
worker result exposed `live-capture canonical item and content disagree` immediately after
submitted-user start correlation.

The generic read correctly requires item-owned `Live` or `Finalized` content. A submitted user item
instead retains the exact ownerless `Sealed` content admitted from its durable draft.

# Why It Failed

Submitted input is correlated provider provenance over an existing canonical user item, not a
provider-created item. Applying the provider-created content proof rejected valid authority and
made healthy capture look incomplete.

# Course Correction

`UserMessage` uses a dedicated typed validation path over the exact submitted item, ownerless sealed
content, expected source identity, lifecycle, disposition, and text equality. Provider-created
items retain the generic stabilized CAS-item/canonical-item/content read. Neither path relaxes the
other or duplicates submitted content.

# Verification

Focused coverage proves ownerless sealed user content through start correlation, completion,
provider `Complete`, terminal audit, projection convergence, concurrency, and close/reopen without
the masked hanging unwind.
