# Scope

Phase 13 provider correlation of a submitted Syndic user item after a projection already exists.

# Invalidated Approach

The first typed correlation mutation advanced the canonical item's revision, CAS source, and
provider lifecycle while leaving its item-projection head and selected transcript authority marked
current at the prior canonical revision.

# Evidence

The all-feature Syndic matrix reached a valid correlate-after-preprojection workflow.
`FinalizeNextTurnItem` rejected the projection because its recorded source-item revision no longer
equaled the correlated canonical item revision. `StartItemProjectionBuild` simultaneously rejected
rebuild because the unchanged head still claimed `Current`.

After mutation invalidation was corrected, positive close/reopen validation exposed a second form
of the same assumption: projection replay categorically rejected every historical `UserInput`
generation even when correlation retained the exact sealed content.

# Why It Failed

The state asserted two incompatible facts: the projection was current, but it did not represent the
current canonical revision. Special-casing finalization would weaken exact revision closure and make
future presentation-affecting source changes indistinguishable from this content-preserving
correlation.

# Course Correction

Provider correlation is a canonical source-revision advance. The same atomic mutation must stale
the affected visible item-projection head and selected transcript authority. Bounded projection
construction may coalesce consecutive correlation/lifecycle advances onto the latest revision,
reuse exact stable content where valid, and publish a generation whose source revision agrees with
the correlated item without reparsing unchanged text. Finalization retains its strict revision
check.

Reopen accepts such a historical user generation only when its content reference exactly equals
current canonical content, the current revision equals the source-event frontier plus the local
pre-correlation revision, and historical revision N replays the exact N-1 correlation frontier.
The pre-correlation and started shapes remain closed; this is not general stale-projection
acceptance.

# Verification

Coverage must exercise user projection before provider correlation, exact start/completion
correlation without duplicate content, stale projection publication in the same commit, bounded
rebuild, terminal finalization, transcript recovery, and reopen agreement.

The focused transcript-construction suite proves both correlation revisions, strict pre-rebuild
finalization refusal, exact stable projection/resource/checkpoint/digest reuse, latest-revision
publication, validation, and close/reopen.

The complete all-feature Syndic suite passes 151 of 151 after stale fixtures were corrected to
select their exact intended identities and valid capture frontiers; no production validator was
weakened for those fixtures.
