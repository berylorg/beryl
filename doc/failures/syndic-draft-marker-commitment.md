# Scope

Phase 168 draft-marker authentication for persistent composer roots, autosave, and
cross-domain CurrentDraft Asset ownership.

# Invalidated Approach

The first proposed design stored the established sequential sealed-content marker digest on every
draft root, recomputed it by replaying the complete piece or marker order after edits, and exposed a
raw caller-constructible summary as the bridge from Syndic roots to Asset publication. Its first
correction added a structural marker-order commitment and bounded seal, but those seal pages carried
only marker id and label while Asset staging required `AssetId`; Asset begin also required the final
sequential summary before Syndic could authenticate it at EOF.

# Decisive Evidence

An ordinary edit near an early marker required work proportional to total piece or marker count.
That contradicted the bounded/logarithmic small-edit, undo, and redo contract. The pure summary also
did not authenticate the exact Syndic draft root or build. After the structural correction, a
one-pass save still could not begin Asset staging until the final summary existed, could not replay
already sealed pages, and had no authenticated marker-to-asset association to append. Retaining the
complete stream, traversing the root twice, or letting the app synthesize associations would violate
the same bounded-memory and cross-domain authority requirements.

# Course Correction

Keep `SequentialMarkerSummaryV1` as the content-neutral ordered marker summary used by Asset sets;
embed it in the content-bound `SealedContentMarkerSummary` used by sealed `ComposerV1`. Give each
draft root a separate persistent structural marker-order commitment whose sequence, identity, and
order leaves all bind the complete `AssetId`. Let Syndic stream a captured exact root once through a
bounded durable seal while deriving both the sequential summary and an independent ordered marker/
asset summary. Couple every seal-page advance with its Asset append in one atomic staging command;
Asset begin accepts no final summary, and Asset seal validates both final summaries. The existing
Syndic mutation participant validates the two opaque proofs inside the final atomic publication
command. Sealed `ComposerV1` remains asset-neutral.

# Affected Work

Reopened Phase 169 and the composer, bounded-resource, Beryl-home storage, image-asset,
`beryl-model`, `syndic-storage`, `beryl-state`, and `beryl-app` design authorities.
