use crate::codec::parts::Encoder;

#[cfg(feature = "test-faults")]
use super::super::DraftEditorCandidateSessionIdV1;
#[cfg(feature = "test-faults")]
use super::super::{DraftCompositePositionV1, DraftPieceOperationIdV1};
use super::super::{DraftPieceDigestV1, DraftPieceRootReferenceV1};
#[cfg(feature = "test-faults")]
use super::codec::authenticated_transition;
#[cfg(feature = "test-faults")]
use super::witness::{DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS, DraftEditHistoryAncestorWitnessV1};
use super::{
    codec::{
        authenticated_frontier, enc_frontier_key, enc_transition_key, encode_frontier_unchecked,
        encode_transition_unchecked,
    },
    records::*,
    references::*,
};

pub fn canonical_empty_draft_edit_history_v1(
    root: DraftPieceRootReferenceV1,
    policy: DraftEditHistoryPolicyV1,
) -> DraftEditHistoryFrontierV1 {
    let provisional = DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            DraftEditHistoryFrontierKeyV1::canonical_empty(root.key().draft_id()),
            0,
            root,
            0,
            policy.byte_budget(),
            policy.revision(),
            DraftEditHistoryAvailabilityV1::NONE,
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        None,
        None,
        None,
        None,
        0,
        0,
        policy.byte_budget(),
        policy.revision(),
    );
    let retained_encoded_bytes =
        stored_frontier_charge(&provisional).expect("canonical history frontier encoding fits u64");
    authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        provisional.reference,
        provisional.journal_head,
        provisional.undo_head,
        provisional.redo_head,
        provisional.oldest_eligible,
        provisional.cumulative_encoded_bytes,
        retained_encoded_bytes,
        provisional.byte_budget,
        provisional.retention_policy_revision,
    ))
}

#[allow(clippy::too_many_arguments)]
#[cfg(feature = "test-faults")]
pub(crate) fn append_ordinary_draft_edit_history_v1(
    source: &DraftEditHistoryFrontierV1,
    successor_generation: u64,
    successor_root: DraftPieceRootReferenceV1,
    before_caret: DraftCompositePositionV1,
    before_selection: DraftCompositePositionV1,
    after_caret: DraftCompositePositionV1,
    after_selection: DraftCompositePositionV1,
    operation_id: DraftPieceOperationIdV1,
) -> Result<(DraftEditHistoryTransitionV1, DraftEditHistoryFrontierV1), DraftEditHistoryAppendErrorV1>
{
    if !source.is_locally_valid() || source.reference.key().session_id().is_none() {
        return Err(DraftEditHistoryAppendErrorV1::InvalidFrontier);
    }
    if source.reference.candidate_generation().checked_add(1) != Some(successor_generation) {
        return Err(DraftEditHistoryAppendErrorV1::GenerationOverflow);
    }
    let frontier_revision = source
        .reference
        .frontier_revision()
        .checked_add(1)
        .ok_or(DraftEditHistoryAppendErrorV1::FrontierRevisionOverflow)?;
    let journal_depth = source
        .journal_head()
        .map_or(Some(1), |head| head.journal_depth().checked_add(1))
        .ok_or(DraftEditHistoryAppendErrorV1::InvalidFrontier)?;
    let mut ancestor_slots = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
    ancestor_slots[0] = source.journal_head();
    let ancestor_witness = DraftEditHistoryAncestorWitnessV1::from_parts(
        super::witness::ancestor_bitmap_for_depth(journal_depth),
        ancestor_slots,
    );
    let provisional_key = DraftEditHistoryTransitionKeyV1::new(
        source.reference.key().draft_id(),
        source
            .reference
            .key()
            .session_id()
            .ok_or(DraftEditHistoryAppendErrorV1::InvalidFrontier)?,
        1,
    );
    let provisional = DraftEditHistoryTransitionV1::from_parts(
        provisional_key,
        source.reference.root(),
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        DraftEditHistoryTransitionKindV1::OrdinaryEdit,
        journal_depth,
        source.journal_head,
        source.undo_head,
        source.redo_head,
        operation_id,
        1,
        ancestor_witness.clone(),
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    let encoded_size = stored_transition_charge(&provisional)?;
    let cumulative_encoded_bytes = source
        .cumulative_encoded_bytes
        .checked_add(encoded_size)
        .ok_or(DraftEditHistoryAppendErrorV1::CumulativePositionOverflow)?;
    let key = DraftEditHistoryTransitionKeyV1::new(
        source.reference.key().draft_id(),
        source
            .reference
            .key()
            .session_id()
            .ok_or(DraftEditHistoryAppendErrorV1::InvalidFrontier)?,
        cumulative_encoded_bytes,
    );
    let transition = authenticated_transition(DraftEditHistoryTransitionV1::from_parts(
        key,
        source.reference.root(),
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        DraftEditHistoryTransitionKindV1::OrdinaryEdit,
        journal_depth,
        source.journal_head,
        source.undo_head,
        source.redo_head,
        operation_id,
        cumulative_encoded_bytes,
        ancestor_witness,
        DraftPieceDigestV1::from_bytes([0; 32]),
    ));
    let transition_reference = transition.reference();
    let provisional_next = DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            source.reference.key(),
            successor_generation,
            successor_root,
            frontier_revision,
            source.byte_budget,
            source.retention_policy_revision,
            DraftEditHistoryAvailabilityV1::new(true, false),
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        Some(transition_reference),
        Some(transition_reference),
        None,
        source.oldest_eligible.or(Some(transition_reference)),
        cumulative_encoded_bytes,
        0,
        source.byte_budget,
        source.retention_policy_revision,
    );
    let source_live_head_charge = stored_frontier_charge(source)?;
    let expected_source_retained = source_live_head_charge
        .checked_add(source.cumulative_encoded_bytes)
        .ok_or(DraftEditHistoryAppendErrorV1::RetainedSizeOverflow)?;
    if source.retained_encoded_bytes != expected_source_retained {
        return Err(DraftEditHistoryAppendErrorV1::InvalidFrontier);
    }
    let retained_encoded_bytes = source
        .retained_encoded_bytes
        .checked_sub(source_live_head_charge)
        .ok_or(DraftEditHistoryAppendErrorV1::InvalidFrontier)?
        .checked_add(stored_frontier_charge(&provisional_next)?)
        .ok_or(DraftEditHistoryAppendErrorV1::RetainedSizeOverflow)?
        .checked_add(encoded_size)
        .ok_or(DraftEditHistoryAppendErrorV1::RetainedSizeOverflow)?;
    if retained_encoded_bytes > source.byte_budget {
        return Err(DraftEditHistoryAppendErrorV1::BudgetExhausted);
    }
    let next = authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        provisional_next.reference,
        provisional_next.journal_head,
        provisional_next.undo_head,
        provisional_next.redo_head,
        provisional_next.oldest_eligible,
        provisional_next.cumulative_encoded_bytes,
        retained_encoded_bytes,
        provisional_next.byte_budget,
        provisional_next.retention_policy_revision,
    ));
    Ok((transition, next))
}

fn checked_encoded_size(encoded_size: u128) -> Result<u64, DraftEditHistoryAppendErrorV1> {
    u64::try_from(encoded_size).map_err(|_| DraftEditHistoryAppendErrorV1::EncodedSizeOverflow)
}

pub(super) fn stored_frontier_charge(
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<u64, DraftEditHistoryAppendErrorV1> {
    let mut key = Encoder::new();
    enc_frontier_key(&mut key, frontier.reference().key());
    stored_record_charge(
        key.finish().len(),
        encode_frontier_unchecked(frontier).len(),
    )
}

pub(super) fn stored_transition_charge(
    transition: &DraftEditHistoryTransitionV1,
) -> Result<u64, DraftEditHistoryAppendErrorV1> {
    let mut key = Encoder::new();
    enc_transition_key(&mut key, transition.key());
    stored_record_charge(
        key.finish().len(),
        encode_transition_unchecked(transition).len(),
    )
}

fn stored_record_charge(
    key_bytes: usize,
    value_bytes: usize,
) -> Result<u64, DraftEditHistoryAppendErrorV1> {
    let encoded_size = (key_bytes as u128)
        .checked_add(value_bytes as u128)
        .ok_or(DraftEditHistoryAppendErrorV1::EncodedSizeOverflow)?;
    checked_encoded_size(encoded_size)
}

#[cfg(feature = "test-faults")]
pub(crate) fn draft_edit_history_stored_charge_components_for_test(
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
) -> Result<[u64; 6], DraftEditHistoryAppendErrorV1> {
    let mut frontier_key = Encoder::new();
    enc_frontier_key(&mut frontier_key, frontier.reference().key());
    let mut transition_key = Encoder::new();
    enc_transition_key(&mut transition_key, transition.key());
    Ok([
        checked_encoded_size(frontier_key.finish().len() as u128)?,
        checked_encoded_size({
            let mut embedded = Encoder::new();
            enc_frontier_key(&mut embedded, frontier.reference().key());
            embedded.finish().len() as u128
        })?,
        checked_encoded_size(encode_frontier_unchecked(frontier).len() as u128)?,
        checked_encoded_size(transition_key.finish().len() as u128)?,
        checked_encoded_size({
            let mut embedded = Encoder::new();
            enc_transition_key(&mut embedded, transition.key());
            embedded.finish().len() as u128
        })?,
        checked_encoded_size(encode_transition_unchecked(transition).len() as u128)?,
    ])
}

#[cfg(feature = "test-faults")]
pub(crate) fn draft_edit_history_overflow_errors_for_test(
    root: DraftPieceRootReferenceV1,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    position: DraftCompositePositionV1,
) -> [DraftEditHistoryAppendErrorV1; 4] {
    fn source(
        root: DraftPieceRootReferenceV1,
        session_id: DraftEditorCandidateSessionIdV1,
        candidate_generation: u64,
        frontier_revision: u64,
        cumulative_encoded_bytes: u64,
    ) -> DraftEditHistoryFrontierV1 {
        let head = (cumulative_encoded_bytes != 0).then(|| {
            DraftEditHistoryTransitionReferenceV1::new(
                DraftEditHistoryTransitionKeyV1::new(
                    root.key().draft_id(),
                    session_id,
                    cumulative_encoded_bytes,
                ),
                cumulative_encoded_bytes,
                1,
                DraftPieceDigestV1::from_bytes([1; 32]),
            )
        });
        authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
            DraftEditHistoryFrontierReferenceV1::new(
                DraftEditHistoryFrontierKeyV1::session(root.key().draft_id(), session_id),
                candidate_generation,
                root,
                frontier_revision,
                u64::MAX,
                1,
                DraftEditHistoryAvailabilityV1::new(head.is_some(), false),
                DraftPieceDigestV1::from_bytes([0; 32]),
            ),
            head,
            head,
            None,
            head,
            cumulative_encoded_bytes,
            0,
            u64::MAX,
            1,
        ))
    }

    let generation = append_ordinary_draft_edit_history_v1(
        &source(root, session_id, u64::MAX, 0, 0),
        0,
        root,
        position,
        position,
        position,
        position,
        operation_id,
    )
    .expect_err("fixture generation must overflow");
    let frontier = append_ordinary_draft_edit_history_v1(
        &source(root, session_id, 0, u64::MAX, 0),
        1,
        root,
        position,
        position,
        position,
        position,
        operation_id,
    )
    .expect_err("fixture frontier revision must overflow");
    let cumulative = append_ordinary_draft_edit_history_v1(
        &source(root, session_id, 0, 0, u64::MAX),
        1,
        root,
        position,
        position,
        position,
        position,
        operation_id,
    )
    .expect_err("fixture cumulative position must overflow");
    let encoded = checked_encoded_size(u128::from(u64::MAX) + 1)
        .expect_err("fixture encoded size must overflow");
    [generation, frontier, cumulative, encoded]
}

#[cfg(feature = "test-faults")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn alternative_ordinary_draft_edit_history_for_test(
    source: &DraftEditHistoryFrontierV1,
    successor_generation: u64,
    successor_root: DraftPieceRootReferenceV1,
    before_caret: DraftCompositePositionV1,
    before_selection: DraftCompositePositionV1,
    after_caret: DraftCompositePositionV1,
    after_selection: DraftCompositePositionV1,
    operation_id: DraftPieceOperationIdV1,
) -> (DraftEditHistoryTransitionV1, DraftEditHistoryFrontierV1) {
    append_ordinary_draft_edit_history_v1(
        source,
        successor_generation,
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        operation_id,
    )
    .expect("fixture alternative history append must be valid")
}
