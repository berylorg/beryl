# Scope

Phase 168 draft-marker authentication for persistent composer roots, autosave, and
cross-domain CurrentDraft Asset ownership.

# Invalidated Approach

The proposed design stored the established sequential sealed-content marker digest on every draft
root, recomputed it by replaying the complete piece or marker order after edits, and exposed the raw
caller-constructible summary as the bridge from Syndic roots to Asset publication.

# Decisive Evidence

An ordinary edit near an early marker required work proportional to total piece or marker count.
That contradicts the authoritative bounded/logarithmic small-edit, undo, and redo contract for
logically unbounded drafts. The pure summary also did not authenticate the exact Syndic draft root
or build that supposedly produced it, so matching summary bytes could not authorize cross-domain
publication.

# Course Correction

Keep `SequentialMarkerSummaryV1` as the content-neutral ordered marker summary used by Asset sets;
embed it in the content-bound `SealedContentMarkerSummary` used by sealed `ComposerV1`. Give each
draft root a separate persistent structural marker-order commitment, then let Syndic stream a
captured exact root through a bounded durable seal and issue an opaque proof binding that
commitment/root to the content-neutral sequential summary. The existing Syndic mutation participant
validates that proof and the matching Asset proof inside one atomic home command.

# Affected Work

Phase 168 and the composer, bounded-resource, Beryl-home storage, image-asset, `beryl-model`,
`syndic-storage`, `beryl-state`, and `beryl-app` design authorities.
