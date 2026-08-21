use beryl_home_store::{DomainReader, HomeStore};

use crate::domain::SyndicDomain;
use crate::mutation::required;
use crate::{SyndicMutationError, SyndicReadError, SyndicStorage};

mod witness;
pub(super) use witness::build_ancestor_witness;
use witness::{transition_is_ancestor_of, transition_is_ancestor_of_read};

#[cfg(feature = "test-faults")]
use super::super::super::DraftPieceDigestV1;
use super::super::super::{
    DraftCompositePositionV1, DraftMarkerIdentityIndexFamily, DraftMarkerIdentityRecordKeyV1,
    DraftMarkerIdentityRecordKindV1, DraftPieceNodesFamily, DraftPieceOperationIdV1,
    DraftPieceRecordKeyV1, DraftPieceRootReferenceV1, DraftPieceRootsFamily, point_limit,
    validate_index_root_record, validate_sequence_root_node,
};
use super::super::{
    append::{stored_frontier_charge, stored_transition_charge},
    codec::DraftEditHistoryTransitionsFamily,
    records::{
        DraftEditHistoryFrontierV1, DraftEditHistoryTransitionKindV1, DraftEditHistoryTransitionV1,
    },
    references::{DraftEditHistoryAvailabilityV1, DraftEditHistoryTransitionReferenceV1},
};

#[cfg(feature = "test-faults")]
mod test_fixtures;
#[cfg(feature = "test-faults")]
use super::super::{
    codec::{authenticated_frontier, authenticated_transition},
    references::{DraftEditHistoryFrontierReferenceV1, DraftEditHistoryTransitionKeyV1},
};
#[cfg(feature = "test-faults")]
pub(crate) use test_fixtures::{
    draft_edit_history_accounting_corruption_for_test,
    draft_edit_history_availability_corruption_for_test,
    draft_edit_history_first_transition_gap_for_test, draft_edit_history_no_head_gap_for_test,
    draft_edit_history_wrong_head_root_for_test,
};

pub(crate) fn authenticate_draft_edit_history_frontier_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<(), SyndicMutationError> {
    if !frontier.is_locally_valid()
        || frontier.undo_head() != frontier.journal_head()
        || frontier.redo_head().is_some()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_root_pin(reader, frontier.reference().root())?;
    let floor = match frontier.oldest_eligible() {
        Some(reference) => Some(exact_transition_reference(reader, reference)?),
        None => None,
    };
    let mut journal = None;
    for reference in [
        frontier.journal_head(),
        frontier.undo_head(),
        frontier.redo_head(),
    ]
    .into_iter()
    .flatten()
    {
        let value = authenticated_transition_reference(reader, reference)?;
        if floor.as_ref().is_some_and(|floor| {
            value.cumulative_encoded_bytes() < floor.cumulative_encoded_bytes()
        }) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_root_pin(reader, value.predecessor_root())?;
        authenticate_root_pin(reader, value.successor_root())?;
        if Some(reference) == frontier.journal_head()
            && value.successor_root() != frontier.reference().root()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if Some(reference) == frontier.journal_head() {
            journal = Some(value);
        }
    }
    if let (Some(head), Some(floor)) = (journal.as_ref(), floor.as_ref())
        && !transition_is_ancestor_of(reader, head, floor)?
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let frontier_charge =
        stored_frontier_charge(frontier).map_err(|_| SyndicMutationError::IdentityCollision)?;
    let retained = match (frontier.journal_head(), floor.as_ref()) {
        (None, None) => frontier_charge,
        (Some(head), Some(floor)) => {
            if head.cumulative_encoded_bytes() != frontier.cumulative_encoded_bytes() {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let floor_charge = stored_transition_charge(floor)
                .map_err(|_| SyndicMutationError::IdentityCollision)?;
            frontier_charge
                .checked_add(
                    frontier
                        .cumulative_encoded_bytes()
                        .checked_sub(
                            floor
                                .cumulative_encoded_bytes()
                                .checked_sub(floor_charge)
                                .ok_or(SyndicMutationError::IdentityCollision)?,
                        )
                        .ok_or(SyndicMutationError::IdentityCollision)?,
                )
                .ok_or(SyndicMutationError::IdentityCollision)?
        }
        _ => return Err(SyndicMutationError::IdentityCollision),
    };
    if retained != frontier.retained_encoded_bytes() || retained > frontier.byte_budget() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

pub(super) fn authenticated_transition_reference(
    reader: &DomainReader<'_, SyndicDomain>,
    reference: DraftEditHistoryTransitionReferenceV1,
) -> Result<DraftEditHistoryTransitionV1, SyndicMutationError> {
    let value = exact_transition_reference(reader, reference)?;
    authenticate_transition_predecessor(reader, &value)?;
    Ok(value)
}

pub(super) fn exact_transition_reference(
    reader: &DomainReader<'_, SyndicDomain>,
    reference: DraftEditHistoryTransitionReferenceV1,
) -> Result<DraftEditHistoryTransitionV1, SyndicMutationError> {
    let value = required::<DraftEditHistoryTransitionsFamily>(reader, &reference.key())?;
    if value.reference() != reference || !value.is_locally_valid() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if value.kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit
        || value.prior_undo() != value.prior_journal()
        || value.prior_redo().is_some()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(value)
}

pub(super) fn authenticate_transition_predecessor(
    reader: &DomainReader<'_, SyndicDomain>,
    value: &DraftEditHistoryTransitionV1,
) -> Result<(), SyndicMutationError> {
    let charge =
        stored_transition_charge(value).map_err(|_| SyndicMutationError::IdentityCollision)?;
    match value.prior_journal() {
        None if value.journal_depth() == 1 && value.cumulative_encoded_bytes() == charge => Ok(()),
        Some(reference) => {
            let prior = required::<DraftEditHistoryTransitionsFamily>(reader, &reference.key())?;
            if prior.reference() != reference
                || !prior.is_locally_valid()
                || prior.successor_root() != value.predecessor_root()
                || prior.journal_depth().checked_add(1) != Some(value.journal_depth())
                || prior.cumulative_encoded_bytes().checked_add(charge)
                    != Some(value.cumulative_encoded_bytes())
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            Ok(())
        }
        _ => Err(SyndicMutationError::IdentityCollision),
    }
}

pub(super) fn authenticate_root_pin(
    reader: &DomainReader<'_, SyndicDomain>,
    root: DraftPieceRootReferenceV1,
) -> Result<(), SyndicMutationError> {
    let stored = required::<DraftPieceRootsFamily>(reader, &root.key())?;
    if stored.reference() != root {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_root_topology(reader, root)
}

pub(super) fn authenticate_root_topology(
    reader: &DomainReader<'_, SyndicDomain>,
    root: DraftPieceRootReferenceV1,
) -> Result<(), SyndicMutationError> {
    if let Some(id) = root.root_node() {
        let node = required::<DraftPieceNodesFamily>(
            reader,
            &DraftPieceRecordKeyV1::new(root.key().draft_id(), id),
        )?;
        validate_sequence_root_node(node, root.summary())
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
    } else if root.summary().piece_count() != 0 {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if let Some(id) = root.marker_index_root() {
        let record = required::<DraftMarkerIdentityIndexFamily>(
            reader,
            &DraftMarkerIdentityRecordKeyV1::new(
                root.key().draft_id(),
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            ),
        )?;
        validate_index_root_record(record, root.marker_index_summary())
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
    } else if root.marker_index_summary().record_count() != 0 {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

pub(crate) fn ordinary_draft_edit_history_adoption_is_locally_exact(
    source: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
    adopted: &DraftEditHistoryFrontierV1,
    before_caret: DraftCompositePositionV1,
    before_selection: DraftCompositePositionV1,
    after_caret: DraftCompositePositionV1,
    after_selection: DraftCompositePositionV1,
    operation_id: DraftPieceOperationIdV1,
) -> bool {
    let Ok(charge) = stored_transition_charge(transition) else {
        return false;
    };
    let Some(cumulative) = source.cumulative_encoded_bytes().checked_add(charge) else {
        return false;
    };
    source.is_locally_valid()
        && transition.is_locally_valid()
        && adopted.is_locally_valid()
        && transition.predecessor_root() == source.reference().root()
        && transition.successor_root() == adopted.reference().root()
        && transition.before_caret() == before_caret
        && transition.before_selection() == before_selection
        && transition.after_caret() == after_caret
        && transition.after_selection() == after_selection
        && transition.operation_id() == operation_id
        && transition.kind() == DraftEditHistoryTransitionKindV1::OrdinaryEdit
        && transition.prior_journal() == source.journal_head()
        && transition.prior_undo() == source.undo_head()
        && transition.prior_redo() == source.redo_head()
        && transition.cumulative_encoded_bytes() == cumulative
        && transition.key().cumulative_encoded_bytes() == cumulative
        && adopted.journal_head() == Some(transition.reference())
        && adopted.undo_head() == Some(transition.reference())
        && adopted.redo_head().is_none()
        && adopted.oldest_eligible().is_some_and(|floor| {
            floor.cumulative_encoded_bytes() <= cumulative
                && source.oldest_eligible().is_none_or(|old| {
                    floor.cumulative_encoded_bytes() >= old.cumulative_encoded_bytes()
                })
        })
        && adopted.cumulative_encoded_bytes() == cumulative
        && adopted.byte_budget() == source.byte_budget()
        && adopted.retention_policy_revision() == source.retention_policy_revision()
        && adopted.reference().candidate_generation()
            == source
                .reference()
                .candidate_generation()
                .checked_add(1)
                .unwrap_or(0)
        && adopted.reference().frontier_revision()
            == source
                .reference()
                .frontier_revision()
                .checked_add(1)
                .unwrap_or(0)
        && adopted.reference().availability() == DraftEditHistoryAvailabilityV1::new(true, false)
        && adopted.retained_encoded_bytes() <= adopted.byte_budget()
}

pub(crate) fn draft_edit_history_frontier_is_authenticated_v1(
    storage: &SyndicStorage,
    store: &HomeStore,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicReadError> {
    if !frontier.is_locally_valid()
        || frontier.undo_head() != frontier.journal_head()
        || frontier.redo_head().is_some()
        || !root_pin_is_authenticated(storage, store, frontier.reference().root())?
    {
        return Ok(false);
    }
    let floor = match frontier.oldest_eligible() {
        Some(reference) => {
            match transition_reference_is_authenticated(storage, store, reference)? {
                Some(value) => Some(value),
                None => return Ok(false),
            }
        }
        None => None,
    };
    let mut journal = None;
    for reference in [
        frontier.journal_head(),
        frontier.undo_head(),
        frontier.redo_head(),
    ]
    .into_iter()
    .flatten()
    {
        let Some(value) = transition_reference_is_authenticated(storage, store, reference)? else {
            return Ok(false);
        };
        if floor.as_ref().is_some_and(|floor| {
            value.cumulative_encoded_bytes() < floor.cumulative_encoded_bytes()
        }) || Some(reference) == frontier.journal_head()
            && value.successor_root() != frontier.reference().root()
        {
            return Ok(false);
        }
        if Some(reference) == frontier.journal_head() {
            journal = Some(value);
        }
    }
    if let (Some(head), Some(floor)) = (journal.as_ref(), floor.as_ref())
        && !transition_is_ancestor_of_read(storage, store, head, floor)?
    {
        return Ok(false);
    }
    let Ok(frontier_charge) = stored_frontier_charge(frontier) else {
        return Ok(false);
    };
    let retained = match (frontier.journal_head(), floor.as_ref()) {
        (None, None) => frontier_charge,
        (Some(head), Some(floor)) => {
            if head.cumulative_encoded_bytes() != frontier.cumulative_encoded_bytes() {
                return Ok(false);
            }
            let Ok(floor_charge) = stored_transition_charge(floor) else {
                return Ok(false);
            };
            let Some(before_floor) = floor.cumulative_encoded_bytes().checked_sub(floor_charge)
            else {
                return Ok(false);
            };
            let Some(transition_bytes) = frontier
                .cumulative_encoded_bytes()
                .checked_sub(before_floor)
            else {
                return Ok(false);
            };
            let Some(retained) = frontier_charge.checked_add(transition_bytes) else {
                return Ok(false);
            };
            retained
        }
        _ => return Ok(false),
    };
    Ok(retained == frontier.retained_encoded_bytes() && retained <= frontier.byte_budget())
}

fn transition_reference_is_authenticated(
    storage: &SyndicStorage,
    store: &HomeStore,
    reference: DraftEditHistoryTransitionReferenceV1,
) -> Result<Option<DraftEditHistoryTransitionV1>, SyndicReadError> {
    let Some(value) = transition_reference_is_exact(storage, store, reference)? else {
        return Ok(None);
    };
    let Ok(charge) = stored_transition_charge(&value) else {
        return Ok(None);
    };
    let predecessor_valid = match value.prior_journal() {
        None if value.journal_depth() == 1 && value.cumulative_encoded_bytes() == charge => true,
        Some(prior_reference) => {
            prior_reference.journal_depth().checked_add(1) == Some(value.journal_depth())
                && prior_reference
                    .cumulative_encoded_bytes()
                    .checked_add(charge)
                    == Some(value.cumulative_encoded_bytes())
        }
        _ => false,
    };
    if !predecessor_valid {
        return Ok(None);
    }
    Ok(Some(value))
}

fn transition_reference_is_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    reference: DraftEditHistoryTransitionReferenceV1,
) -> Result<Option<DraftEditHistoryTransitionV1>, SyndicReadError> {
    let value = storage.point::<DraftEditHistoryTransitionsFamily>(
        store,
        reference.key(),
        point_limit(),
    )?;
    let Some(value) = value.filter(|value| {
        value.reference() == reference
            && value.is_locally_valid()
            && value.kind() == DraftEditHistoryTransitionKindV1::OrdinaryEdit
            && value.prior_undo() == value.prior_journal()
            && value.prior_redo().is_none()
    }) else {
        return Ok(None);
    };
    Ok(Some(value))
}

fn root_pin_is_authenticated(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
) -> Result<bool, SyndicReadError> {
    let Some(stored) = storage.point::<DraftPieceRootsFamily>(store, root.key(), point_limit())?
    else {
        return Ok(false);
    };
    if stored.reference() != root {
        return Ok(false);
    }
    if let Some(id) = root.root_node() {
        let Some(node) = storage.point::<DraftPieceNodesFamily>(
            store,
            DraftPieceRecordKeyV1::new(root.key().draft_id(), id),
            point_limit(),
        )?
        else {
            return Ok(false);
        };
        if validate_sequence_root_node(node, root.summary()).is_err() {
            return Ok(false);
        }
    } else if root.summary().piece_count() != 0 {
        return Ok(false);
    }
    if let Some(id) = root.marker_index_root() {
        let Some(record) = storage.point::<DraftMarkerIdentityIndexFamily>(
            store,
            DraftMarkerIdentityRecordKeyV1::new(
                root.key().draft_id(),
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            ),
            point_limit(),
        )?
        else {
            return Ok(false);
        };
        if validate_index_root_record(record, root.marker_index_summary()).is_err() {
            return Ok(false);
        }
    } else if root.marker_index_summary().record_count() != 0 {
        return Ok(false);
    }
    Ok(true)
}
