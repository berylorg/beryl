mod authentication;

pub(crate) use authentication::authenticate_root_topology;
use authentication::transition_is_ancestor_of;
pub(crate) use authentication::{
    authenticate_draft_edit_history_frontier_v1, draft_edit_history_frontier_is_authenticated_v1,
    ordinary_draft_edit_history_adoption_is_locally_exact,
};
use authentication::{
    authenticated_transition_reference, build_ancestor_witness, exact_transition_reference,
};
#[cfg(feature = "test-faults")]
pub(crate) use authentication::{
    draft_edit_history_accounting_corruption_for_test,
    draft_edit_history_availability_corruption_for_test,
    draft_edit_history_first_transition_gap_for_test, draft_edit_history_no_head_gap_for_test,
    draft_edit_history_wrong_head_root_for_test,
};

use beryl_home_store::DomainReader;

use crate::SyndicMutationError;
use crate::domain::SyndicDomain;

use super::super::{
    DraftCompositePositionV1, DraftEditorCandidateSessionIdV1, DraftPieceDigestV1,
    DraftPieceOperationIdV1, DraftPieceRootReferenceV1,
};
use super::{
    append::{stored_frontier_charge, stored_transition_charge},
    codec::{authenticated_frontier, authenticated_transition},
    records::{
        DraftEditHistoryAppendErrorV1, DraftEditHistoryFrontierV1,
        DraftEditHistoryTransitionKindV1, DraftEditHistoryTransitionV1,
    },
    references::{
        DraftEditHistoryAvailabilityV1, DraftEditHistoryFrontierReferenceV1,
        DraftEditHistoryTransitionKeyV1, DraftEditHistoryTransitionReferenceV1,
    },
    witness::{DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS, DraftEditHistoryAncestorWitnessV1},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftEditHistoryRetentionErrorV1 {
    CapacityUnavailable,
    Invalid,
}

impl From<DraftEditHistoryAppendErrorV1> for DraftEditHistoryRetentionErrorV1 {
    fn from(_: DraftEditHistoryAppendErrorV1) -> Self {
        Self::Invalid
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn append_ordinary_draft_edit_history_with_retention_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &DraftEditHistoryFrontierV1,
    successor_generation: u64,
    successor_root: DraftPieceRootReferenceV1,
    before_caret: DraftCompositePositionV1,
    before_selection: DraftCompositePositionV1,
    after_caret: DraftCompositePositionV1,
    after_selection: DraftCompositePositionV1,
    operation_id: DraftPieceOperationIdV1,
) -> Result<
    (DraftEditHistoryTransitionV1, DraftEditHistoryFrontierV1),
    DraftEditHistoryRetentionErrorV1,
> {
    authenticate_draft_edit_history_frontier_v1(reader, source)
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    authenticate_root_topology(reader, successor_root)
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    let session_id = source
        .reference()
        .key()
        .session_id()
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    if source.reference().candidate_generation().checked_add(1) != Some(successor_generation) {
        return Err(DraftEditHistoryRetentionErrorV1::Invalid);
    }
    let frontier_revision = source
        .reference()
        .frontier_revision()
        .checked_add(1)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let (journal_depth, ancestor_witness) =
        build_ancestor_witness(reader, source.journal_head())
            .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    let provisional = transition(
        source,
        session_id,
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        DraftEditHistoryTransitionKindV1::OrdinaryEdit,
        operation_id,
        1,
        journal_depth,
        ancestor_witness.clone(),
    );
    let transition_charge = stored_transition_charge(&provisional)?;
    let cumulative = source
        .cumulative_encoded_bytes()
        .checked_add(transition_charge)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let transition = transition(
        source,
        session_id,
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        DraftEditHistoryTransitionKindV1::OrdinaryEdit,
        operation_id,
        cumulative,
        journal_depth,
        ancestor_witness,
    );
    if stored_transition_charge(&transition)? != transition_charge {
        return Err(DraftEditHistoryRetentionErrorV1::Invalid);
    }
    let transition_reference = transition.reference();
    let provisional_next = DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            source.reference().key(),
            successor_generation,
            successor_root,
            frontier_revision,
            source.byte_budget(),
            source.retention_policy_revision(),
            DraftEditHistoryAvailabilityV1::new(true, false),
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        Some(transition_reference),
        Some(transition_reference),
        None,
        Some(transition_reference),
        cumulative,
        0,
        source.byte_budget(),
        source.retention_policy_revision(),
    );
    let next_frontier_charge = stored_frontier_charge(&provisional_next)?;
    let required = next_frontier_charge
        .checked_add(transition_charge)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    if required > source.byte_budget() {
        return Err(DraftEditHistoryRetentionErrorV1::CapacityUnavailable);
    }
    let source_frontier_charge = stored_frontier_charge(source)?;
    let retained_without_source_head = source
        .retained_encoded_bytes()
        .checked_sub(source_frontier_charge)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let retain_all = retained_without_source_head
        .checked_add(next_frontier_charge)
        .and_then(|value| value.checked_add(transition_charge))
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let (oldest, oldest_charge, retained) = if retain_all <= source.byte_budget() {
        let oldest = source.oldest_eligible().unwrap_or(transition_reference);
        let oldest_charge = if source.oldest_eligible().is_some() {
            authenticated_transition_reference(reader, oldest)
                .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)
                .and_then(|value| stored_transition_charge(&value).map_err(Into::into))?
        } else {
            transition_charge
        };
        (oldest, oldest_charge, retain_all)
    } else {
        let transition_allowance = source
            .byte_budget()
            .checked_sub(next_frontier_charge)
            .ok_or(DraftEditHistoryRetentionErrorV1::CapacityUnavailable)?;
        let threshold = cumulative
            .checked_sub(transition_allowance)
            .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
        let selected = oldest_retained_at_threshold(reader, source, &transition, threshold)
            .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
        let oldest_charge = stored_transition_charge(&selected)?;
        let oldest = selected.reference();
        let retained_transition_bytes = cumulative
            .checked_sub(
                oldest
                    .cumulative_encoded_bytes()
                    .checked_sub(oldest_charge)
                    .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?,
            )
            .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
        let retained = next_frontier_charge
            .checked_add(retained_transition_bytes)
            .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
        (oldest, oldest_charge, retained)
    };
    let retained_transition_bytes = cumulative
        .checked_sub(
            oldest
                .cumulative_encoded_bytes()
                .checked_sub(oldest_charge)
                .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?,
        )
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    if next_frontier_charge.checked_add(retained_transition_bytes) != Some(retained)
        || retained > source.byte_budget()
    {
        return Err(DraftEditHistoryRetentionErrorV1::Invalid);
    }
    let next = authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
        provisional_next.reference,
        provisional_next.journal_head,
        provisional_next.undo_head,
        provisional_next.redo_head,
        Some(oldest),
        cumulative,
        retained,
        provisional_next.byte_budget,
        provisional_next.retention_policy_revision,
    ));
    Ok((transition, next))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn append_historical_draft_edit_history_with_retention_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &DraftEditHistoryFrontierV1,
    selected_reference: DraftEditHistoryTransitionReferenceV1,
    kind: DraftEditHistoryTransitionKindV1,
    successor_generation: u64,
    successor_root: DraftPieceRootReferenceV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
    operation_id: DraftPieceOperationIdV1,
) -> Result<
    (DraftEditHistoryTransitionV1, DraftEditHistoryFrontierV1),
    DraftEditHistoryRetentionErrorV1,
> {
    authenticate_draft_edit_history_frontier_v1(reader, source)
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    authenticate_root_topology(reader, successor_root)
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    if !matches!(
        kind,
        DraftEditHistoryTransitionKindV1::Undo | DraftEditHistoryTransitionKindV1::Redo
    ) || source.reference().candidate_generation().checked_add(1) != Some(successor_generation)
    {
        return Err(DraftEditHistoryRetentionErrorV1::Invalid);
    }
    let selected = exact_transition_reference(reader, selected_reference)
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    let expected_selected = match kind {
        DraftEditHistoryTransitionKindV1::Undo => source.undo_head(),
        DraftEditHistoryTransitionKindV1::Redo => source.redo_head(),
        DraftEditHistoryTransitionKindV1::OrdinaryEdit => None,
    };
    let head = source
        .journal_head()
        .map(|reference| exact_transition_reference(reader, reference))
        .transpose()
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    if expected_selected != Some(selected_reference)
        || head
            .as_ref()
            .is_none_or(|head| !transition_is_ancestor_of(reader, head, &selected).unwrap_or(false))
        || selected.successor_root() != source.reference().root()
        || selected.predecessor_root() != successor_root
        || selected.before_caret() != caret
        || selected.before_selection() != selection
    {
        return Err(DraftEditHistoryRetentionErrorV1::Invalid);
    }
    let session_id = source
        .reference()
        .key()
        .session_id()
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let frontier_revision = source
        .reference()
        .frontier_revision()
        .checked_add(1)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let (journal_depth, ancestor_witness) =
        build_ancestor_witness(reader, source.journal_head())
            .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    let provisional = transition(
        source,
        session_id,
        successor_root,
        selected.after_caret(),
        selected.after_selection(),
        caret,
        selection,
        kind,
        operation_id,
        1,
        journal_depth,
        ancestor_witness.clone(),
    );
    let transition_charge = stored_transition_charge(&provisional)?;
    let cumulative = source
        .cumulative_encoded_bytes()
        .checked_add(transition_charge)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let transition = transition(
        source,
        session_id,
        successor_root,
        selected.after_caret(),
        selected.after_selection(),
        caret,
        selection,
        kind,
        operation_id,
        cumulative,
        journal_depth,
        ancestor_witness,
    );
    if stored_transition_charge(&transition)? != transition_charge {
        return Err(DraftEditHistoryRetentionErrorV1::Invalid);
    }
    let transition_reference = transition.reference();
    let (candidate_undo, candidate_redo) = match kind {
        DraftEditHistoryTransitionKindV1::Undo => {
            (selected.prior_undo(), Some(transition_reference))
        }
        DraftEditHistoryTransitionKindV1::Redo => {
            (Some(transition_reference), selected.prior_redo())
        }
        DraftEditHistoryTransitionKindV1::OrdinaryEdit => unreachable!(),
    };
    let provisional_next = |undo: Option<DraftEditHistoryTransitionReferenceV1>,
                            redo: Option<DraftEditHistoryTransitionReferenceV1>,
                            oldest: DraftEditHistoryTransitionReferenceV1,
                            retained: u64| {
        let availability = DraftEditHistoryAvailabilityV1::new(undo.is_some(), redo.is_some());
        authenticated_frontier(DraftEditHistoryFrontierV1::from_parts(
            DraftEditHistoryFrontierReferenceV1::new(
                source.reference().key(),
                successor_generation,
                successor_root,
                frontier_revision,
                source.byte_budget(),
                source.retention_policy_revision(),
                availability,
                DraftPieceDigestV1::from_bytes([0; 32]),
            ),
            Some(transition_reference),
            undo,
            redo,
            Some(oldest),
            cumulative,
            retained,
            source.byte_budget(),
            source.retention_policy_revision(),
        ))
    };
    let source_frontier_charge = stored_frontier_charge(source)?;
    let retained_without_source_head = source
        .retained_encoded_bytes()
        .checked_sub(source_frontier_charge)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let initial_oldest = source.oldest_eligible().unwrap_or(transition_reference);
    let initial = provisional_next(candidate_undo, candidate_redo, initial_oldest, 0);
    let initial_head_charge = stored_frontier_charge(&initial)?;
    if initial_head_charge
        .checked_add(transition_charge)
        .is_none_or(|required| required > source.byte_budget())
    {
        return Err(DraftEditHistoryRetentionErrorV1::CapacityUnavailable);
    }
    let retain_all = retained_without_source_head
        .checked_add(initial_head_charge)
        .and_then(|value| value.checked_add(transition_charge))
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let oldest = if retain_all <= source.byte_budget() {
        initial_oldest
    } else {
        let allowance = source
            .byte_budget()
            .checked_sub(initial_head_charge)
            .ok_or(DraftEditHistoryRetentionErrorV1::CapacityUnavailable)?;
        let threshold = cumulative
            .checked_sub(allowance)
            .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
        oldest_retained_at_threshold(reader, source, &transition, threshold)
            .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?
            .reference()
    };
    let retained_link = |reference: Option<DraftEditHistoryTransitionReferenceV1>| {
        reference.filter(|reference| {
            reference.journal_depth() >= oldest.journal_depth()
                && reference.cumulative_encoded_bytes() >= oldest.cumulative_encoded_bytes()
        })
    };
    let undo = retained_link(candidate_undo);
    let redo = retained_link(candidate_redo);
    let provisional_final = provisional_next(undo, redo, oldest, 0);
    let final_head_charge = stored_frontier_charge(&provisional_final)?;
    let oldest_transition = exact_transition_reference(reader, oldest)
        .or_else(|_| {
            if oldest == transition_reference {
                Ok(transition.clone())
            } else {
                Err(SyndicMutationError::IdentityCollision)
            }
        })
        .map_err(|_| DraftEditHistoryRetentionErrorV1::Invalid)?;
    let oldest_charge = stored_transition_charge(&oldest_transition)?;
    let retained_transition_bytes = cumulative
        .checked_sub(
            oldest
                .cumulative_encoded_bytes()
                .checked_sub(oldest_charge)
                .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?,
        )
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    let retained = final_head_charge
        .checked_add(retained_transition_bytes)
        .ok_or(DraftEditHistoryRetentionErrorV1::Invalid)?;
    if retained > source.byte_budget() {
        return Err(DraftEditHistoryRetentionErrorV1::CapacityUnavailable);
    }
    Ok((transition, provisional_next(undo, redo, oldest, retained)))
}

#[allow(clippy::too_many_arguments)]
fn transition(
    source: &DraftEditHistoryFrontierV1,
    session_id: DraftEditorCandidateSessionIdV1,
    successor_root: DraftPieceRootReferenceV1,
    before_caret: DraftCompositePositionV1,
    before_selection: DraftCompositePositionV1,
    after_caret: DraftCompositePositionV1,
    after_selection: DraftCompositePositionV1,
    kind: DraftEditHistoryTransitionKindV1,
    operation_id: DraftPieceOperationIdV1,
    cumulative: u64,
    journal_depth: u64,
    ancestor_witness: DraftEditHistoryAncestorWitnessV1,
) -> DraftEditHistoryTransitionV1 {
    authenticated_transition(DraftEditHistoryTransitionV1::from_parts(
        DraftEditHistoryTransitionKeyV1::new(
            source.reference().key().draft_id(),
            session_id,
            cumulative,
        ),
        source.reference().root(),
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        kind,
        journal_depth,
        source.journal_head(),
        source.undo_head(),
        source.redo_head(),
        operation_id,
        cumulative,
        ancestor_witness,
        DraftPieceDigestV1::from_bytes([0; 32]),
    ))
}

fn oldest_retained_at_threshold(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &DraftEditHistoryFrontierV1,
    head: &DraftEditHistoryTransitionV1,
    threshold: u64,
) -> Result<DraftEditHistoryTransitionV1, SyndicMutationError> {
    let floor_depth = source
        .oldest_eligible()
        .map_or(1, |value| value.journal_depth());
    let mut selected = head.clone();
    for level in (0..DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS).rev() {
        let Some(reference) = selected.ancestor_witness().ancestor(level) else {
            continue;
        };
        if reference.journal_depth() < floor_depth {
            continue;
        }
        let candidate = exact_transition_reference(reader, reference)?;
        let candidate_charge = stored_transition_charge(&candidate)
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        let candidate_start = candidate
            .cumulative_encoded_bytes()
            .checked_sub(candidate_charge)
            .ok_or(SyndicMutationError::IdentityCollision)?;
        if candidate_start >= threshold {
            selected = candidate;
        }
    }
    let selected_charge =
        stored_transition_charge(&selected).map_err(|_| SyndicMutationError::IdentityCollision)?;
    if selected
        .cumulative_encoded_bytes()
        .checked_sub(selected_charge)
        .is_none_or(|start| start < threshold)
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(selected)
}
