use super::*;
use crate::DraftEditHistoryAncestorWitnessV1;

pub(crate) fn draft_edit_history_accounting_corruption_for_test(
    frontier: &DraftEditHistoryFrontierV1,
) -> DraftEditHistoryFrontierV1 {
    authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        frontier.reference,
        frontier.journal_head,
        frontier.undo_head,
        frontier.redo_head,
        frontier.oldest_eligible,
        frontier.cumulative_encoded_bytes,
        frontier.retained_encoded_bytes.saturating_sub(1),
        frontier.byte_budget,
        frontier.retention_policy_revision,
    ))
}

pub(crate) fn draft_edit_history_availability_corruption_for_test(
    frontier: &DraftEditHistoryFrontierV1,
    undo_head: DraftEditHistoryTransitionReferenceV1,
) -> DraftEditHistoryFrontierV1 {
    let reference = frontier.reference();
    authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            reference.key(),
            reference.candidate_generation(),
            reference.root(),
            reference.frontier_revision(),
            reference.byte_budget(),
            reference.retention_policy_revision(),
            DraftEditHistoryAvailabilityV1::new(true, reference.availability().redo_available()),
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        frontier.journal_head,
        Some(undo_head),
        frontier.redo_head,
        frontier.oldest_eligible,
        frontier.cumulative_encoded_bytes,
        frontier.retained_encoded_bytes,
        frontier.byte_budget,
        frontier.retention_policy_revision,
    ))
}

pub(crate) fn draft_edit_history_no_head_gap_for_test(
    frontier: &DraftEditHistoryFrontierV1,
) -> DraftEditHistoryFrontierV1 {
    authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        frontier.reference,
        None,
        None,
        None,
        None,
        1,
        frontier.retained_encoded_bytes,
        frontier.byte_budget,
        frontier.retention_policy_revision,
    ))
}

pub(crate) fn draft_edit_history_first_transition_gap_for_test(
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
) -> (DraftEditHistoryFrontierV1, DraftEditHistoryTransitionV1) {
    let cumulative = transition.cumulative_encoded_bytes() + 1;
    let replacement_transition =
        authenticated_transition(DraftEditHistoryTransitionV1::from_parts(
            DraftEditHistoryTransitionKeyV1::new(
                transition.key().draft_id(),
                transition.key().session_id(),
                cumulative,
            ),
            transition.predecessor_root(),
            transition.successor_root(),
            transition.before_caret(),
            transition.before_selection(),
            transition.after_caret(),
            transition.after_selection(),
            transition.kind(),
            1,
            None,
            None,
            None,
            transition.operation_id(),
            cumulative,
            DraftEditHistoryAncestorWitnessV1::EMPTY,
            DraftPieceDigestV1::from_bytes([0; 32]),
        ));
    let reference = frontier.reference();
    let replacement_frontier = authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            reference.key(),
            reference.candidate_generation(),
            reference.root(),
            reference.frontier_revision(),
            reference.byte_budget(),
            reference.retention_policy_revision(),
            DraftEditHistoryAvailabilityV1::new(true, false),
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        Some(replacement_transition.reference()),
        Some(replacement_transition.reference()),
        None,
        Some(replacement_transition.reference()),
        cumulative,
        frontier.retained_encoded_bytes,
        frontier.byte_budget,
        frontier.retention_policy_revision,
    ));
    (replacement_frontier, replacement_transition)
}
pub(crate) fn draft_edit_history_wrong_head_root_for_test(
    frontier: &DraftEditHistoryFrontierV1,
    head: &DraftEditHistoryTransitionV1,
) -> DraftEditHistoryFrontierV1 {
    let reference = frontier.reference();
    let provisional = authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            reference.key(),
            reference.candidate_generation(),
            reference.root(),
            reference.frontier_revision(),
            reference.byte_budget(),
            reference.retention_policy_revision(),
            reference.availability(),
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        Some(head.reference()),
        Some(head.reference()),
        None,
        Some(head.reference()),
        head.cumulative_encoded_bytes(),
        0,
        frontier.byte_budget,
        frontier.retention_policy_revision,
    ));
    let retained =
        stored_frontier_charge(&provisional).unwrap() + stored_transition_charge(head).unwrap();
    authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        provisional.reference,
        provisional.journal_head,
        provisional.undo_head,
        provisional.redo_head,
        provisional.oldest_eligible,
        provisional.cumulative_encoded_bytes,
        retained,
        provisional.byte_budget,
        provisional.retention_policy_revision,
    ))
}
