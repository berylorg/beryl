use beryl_model::{DraftRevision, SyndicDraftId};

use super::*;
use crate::draft_piece::{
    DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftEditorCandidateSessionIdV1,
    DraftPieceOperationIdV1, canonical_empty_draft_piece_root_v1,
};

fn transition(
    depth: u64,
    prior: Option<DraftEditHistoryTransitionReferenceV1>,
    second_ancestor: Option<DraftEditHistoryTransitionReferenceV1>,
    digest: DraftPieceDigestV1,
) -> DraftEditHistoryTransitionV1 {
    let draft_id = SyndicDraftId::from_bytes([11; 16]);
    let operation_id = DraftPieceOperationIdV1::from_bytes([12; 16]);
    let root =
        canonical_empty_draft_piece_root_v1(draft_id, DraftRevision::new(1).unwrap(), operation_id)
            .reference();
    let mut slots = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
    slots[0] = prior;
    slots[1] = second_ancestor;
    DraftEditHistoryTransitionV1::from_parts(
        DraftEditHistoryTransitionKeyV1::new(
            draft_id,
            DraftEditorCandidateSessionIdV1::from_bytes([13; 16]),
            depth * 1_000,
        ),
        root,
        root,
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous),
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous),
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous),
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous),
        DraftEditHistoryTransitionKindV1::OrdinaryEdit,
        depth,
        prior,
        prior,
        None,
        operation_id,
        depth * 1_000,
        DraftEditHistoryAncestorWitnessV1::from_parts(ancestor_bitmap_for_depth(depth), slots),
        digest,
    )
}

fn authenticated_chain() -> [DraftEditHistoryTransitionV1; 3] {
    let first = authenticated_transition(transition(
        1,
        None,
        None,
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    let second = authenticated_transition(transition(
        2,
        Some(first.reference()),
        None,
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    let third = authenticated_transition(transition(
        3,
        Some(second.reference()),
        Some(first.reference()),
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    [first, second, third]
}

#[test]
fn witness_round_trips_and_commits_every_reference_byte() {
    let [first, second, third] = authenticated_chain();
    let encoded = DraftEditHistoryTransitionsFamily::encode_value(&third).unwrap();
    assert_eq!(
        DraftEditHistoryTransitionsFamily::decode_value(&encoded).unwrap(),
        third
    );
    let original_digest = third.digest();
    let replaced = DraftEditHistoryTransitionReferenceV1::new(
        first.key(),
        first.cumulative_encoded_bytes(),
        first.journal_depth(),
        DraftPieceDigestV1::from_bytes([99; 32]),
    );
    let replacement = authenticated_transition(transition(
        3,
        Some(second.reference()),
        Some(replaced),
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    assert_ne!(replacement.digest(), original_digest);
    assert_ne!(
        DraftEditHistoryTransitionsFamily::encode_value(&replacement).unwrap(),
        encoded
    );
}

#[test]
fn missing_wrong_level_and_stale_digest_witness_bytes_fail_decode() {
    let [first, second, third] = authenticated_chain();
    let missing = authenticated_transition(transition(
        3,
        Some(second.reference()),
        None,
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    assert!(
        DraftEditHistoryTransitionsFamily::decode_value(&encode_transition_unchecked(&missing))
            .is_err()
    );
    let wrong_level = authenticated_transition(transition(
        3,
        Some(second.reference()),
        Some(second.reference()),
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    assert!(
        DraftEditHistoryTransitionsFamily::decode_value(&encode_transition_unchecked(&wrong_level))
            .is_err()
    );
    let stale_digest = transition(
        3,
        Some(second.reference()),
        Some(DraftEditHistoryTransitionReferenceV1::new(
            first.key(),
            first.cumulative_encoded_bytes(),
            first.journal_depth(),
            DraftPieceDigestV1::from_bytes([77; 32]),
        )),
        third.digest(),
    );
    assert!(
        DraftEditHistoryTransitionsFamily::decode_value(&encode_transition_unchecked(
            &stale_digest
        ))
        .is_err()
    );
}
