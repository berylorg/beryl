# Text Insertion Across Marker-Bearing Sequence Splits

Scope: bounded draft-piece text insertion and sequence-tree progress validation.

## Reproduction

App qualification on 2026-09-15 reached this production failure after cooperative content reuse
was accepted as `1ccd6cb1`. The unchanged submitted-input scale test completed all four
marker-free shapes, including repeated identical content, and the 16-image shape. The 64-image
shape failed while appending text after its 64th marker, before submission or execution.

The fixture uses separate authenticated marker-only and text-only operations, bounded text
fragments, replayed evidence and staging passes, and an exact after-marker source gap. Draft seed
156, session seed 45, and operation 129 have predecessor candidate generation 128 with 192 pieces,
64 markers and 335,872 UTF-8 bytes. The first 2,624-byte text fragment reaches authenticated
`Inserting` progress at rank 192, then returns `MutationBuildPreparation(Build(InvalidRoot))`.
Independent review confirmed that this is a valid fixture operation.

## Cause

The rightmost 128-child sequence node splits when text leaf 193 is appended. Leaf grouping changes
from `64|128` to `64|64|65`; the corresponding marker groups change from `21|43` to `21|21|22`.
The sequence summary's marker digest recursively hashes child marker digests and child count.
That fold is shape-dependent and nonassociative, so this split changes it without changing marker
identity, order, labels, assets or count.

`draft_piece/tree/mapping_progress/splice.rs` nevertheless requires the before and after sequence
marker digests to be equal for text insertion. Its other checks already preserve the independent
marker identity index, order root, commitment and count. The extra digest equality rejects the
valid split. The earlier height growth at leaf 129 was a marker insertion, explaining why this
text-only check first rejects operation 129 rather than that earlier tree change.

The owning V7 text-only rebase contract preserves complete marker identity-index and order roots;
it does not require a topology-dependent sequence fold to remain unchanged. Independent source
review and the captured runtime frontier agree on this contradiction. The materializer's output
replay and canonical content identity are separate and are not implicated.

## Accepted Correction

Removed only the cross-transition sequence marker-digest equality from text insertion and text
removal progress validation. Complete marker identity and order roots, summaries, count, order
height, commitment and effect continuation remain unchanged requirements. Persisted encodings
and digest formulas are unchanged.

Independent review traced bounded construction: insertion copies unchanged authenticated children
in order and permits interior splits only in text leaves; text removal requires a text leaf and
retains surviving children in order through merge and root collapse. Every selected sequence root
still validates its marker digest against its own computed node summary. Cross-transition equality
was invalid; this local root authentication remains required.

Reopen validates selected and predecessor receipts, root descriptors, effects and cursor/summary
deltas. It does not repeat prior tree surgery or scan every unchanged marker. Immutable atomic
publication establishes derivation under the existing draft-storage trust boundary; this correction
does not claim protection against coordinated substitution of all same-database authority anchors.

## Verification

Accepted on 2026-09-15 after Operator authorization. The new regression reproduced `InvalidRoot`
at bounded insertion step 13 before the correction. All 57 selected cases across `piece_tree`,
`build_mapping_continuation`, `build_mapping_custody` and `staged_build_outcomes` passed: 56 in the
combined run and the corrected corruption assertion in a targeted rerun. Normal library compilation,
formatting and independent adversarial review passed.

New cases cover root growth, a descendant split with two markers distributed across the new
children, an interior UTF-8/newline insertion, removal reshaping, and physical reopen immediately
after every observed digest change. They preserve complete marker authorities and exact occurrence
facts including marker leaf identities and digests, verify marker positions and final text, and
bound each observed advance to 256 staged records. Substituted sequence, marker-index and
marker-order root records refuse further advancement. When revision reads remain available, they
show no new mutation; corruption may instead fail the home. A deliberately corrupted shared marker
root also invalidates predecessor reads, so those reads are not an unaffected-state oracle.

App qualification resumes with its original scale inputs. This storage acceptance alone does not
accept app construction, the outstanding failure-taxonomy test or executable bootstrap.
