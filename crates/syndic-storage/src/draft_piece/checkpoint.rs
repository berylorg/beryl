use beryl_home_store::{DomainReader, HomeStore};

use crate::codec::Family;
use crate::domain::{SyndicDomain, SyndicStorage};
use crate::{SyndicMutationError, SyndicReadError};

use super::*;

mod historical;
mod reader;
mod saved;
mod transition;

use reader::{CheckpointReader, StoreCheckpointReader};

pub(super) fn candidate_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicReadError> {
    candidate_is_exact(&StoreCheckpointReader { storage, store }, head, frontier)
}

pub(super) fn candidate_is_exact_in_transaction(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicMutationError> {
    candidate_is_exact(reader, head, frontier)
}

pub(super) fn opening_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicReadError> {
    opening_is_exact(&StoreCheckpointReader { storage, store }, head, frontier)
}

pub(super) fn opening_is_exact_in_transaction(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicMutationError> {
    opening_is_exact(reader, head, frontier)
}

pub(super) fn has_opening_identity(head: &DraftEditorCandidateSessionV1) -> bool {
    head.newest_candidate_generation() == head.durable_base_history().candidate_generation()
        && head.newest_root() == head.durable_base_root()
        && head.newest_history().key()
            == DraftEditHistoryFrontierKeyV1::session(head.draft_id(), head.session_id())
        && head.dirty_generation() == 0
}

pub(crate) fn has_saved_identity(head: &DraftEditorCandidateSessionV1) -> bool {
    head.published_candidate_generation() == head.newest_candidate_generation()
        && head.published_root() == head.newest_root()
        && (head.published_history() == head.newest_history()
            || has_opening_identity(head)
                && head.published_selector_revision() == head.durable_base_selector_revision()
                && head.published_root() == head.durable_base_root()
                && head.published_history() == head.durable_base_history())
}

pub(super) fn saved_checkpoint_is_exact_in_transaction(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicMutationError> {
    if !has_saved_identity(head)
        || !publication::candidate_session_publication_is_exact(reader, head)?
        || !root_and_frontier_are_exact(reader, head, frontier)?
    {
        return Ok(false);
    }
    if head.published_history() == head.newest_history() {
        return Ok(true);
    }
    opening_is_exact(reader, head, frontier)
}

pub(super) fn disposed_saved_checkpoint(
    head: &DraftEditorCandidateSessionV1,
    operation_id: DraftPieceOperationIdV1,
) -> Option<DraftEditorCandidateSessionV1> {
    if head.published_history() == head.newest_history() {
        head.disposed(operation_id)
    } else {
        head.disposed_opening(operation_id)
    }
}

fn root_and_frontier_are_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    if frontier.reference() != head.newest_history()
        || frontier.reference().root() != head.newest_root()
        || frontier.reference().candidate_generation() != head.newest_candidate_generation()
    {
        return Ok(false);
    }
    let Some(root) = reader.point::<DraftPieceRootsFamily>(&head.newest_root().key())? else {
        return Ok(false);
    };
    Ok(root.reference() == head.newest_root()
        && draft_piece_root_reference_is_locally_exact_v1(root.reference())
        && reader.authenticate_frontier(frontier)?)
}

fn opening_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    if !has_opening_identity(head) || frontier.reference() != head.newest_history() {
        return Ok(false);
    }
    let key = DraftEditorCandidateSessionRecordKeyV1::open_receipt(
        head.draft_id(),
        head.session_id(),
        head.open_operation_id(),
    );
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(open)) =
        reader.point::<DraftEditorCandidateSessionsFamily>(&key)?
    else {
        return Ok(false);
    };
    if !session::receipt_matches_head(&open, head)
        || open.head().newest_candidate_generation() != head.newest_candidate_generation()
        || open.head().newest_root() != head.newest_root()
        || open.head().newest_history() != head.newest_history()
    {
        return Ok(false);
    }
    let Some(durable) =
        reader.point::<DraftEditHistoryFrontiersFamily>(&head.durable_base_history().key())?
    else {
        return Ok(false);
    };
    Ok(durable.reference() == head.durable_base_history()
        && durable.fork_session(head.session_id()).as_ref() == Some(frontier)
        && reader.authenticate_frontier(&durable)?
        && reader.authenticate_frontier(frontier)?)
}

fn candidate_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    if !root_and_frontier_are_exact(reader, head, frontier)? {
        return Ok(false);
    }
    candidate_transition_is_exact(reader, head, frontier)
}

#[inline(never)]
fn candidate_transition_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    if has_opening_identity(head) {
        return opening_is_exact(reader, head, frontier);
    }
    transition::transition_is_exact(reader, head, frontier)
}

fn historical_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
) -> Result<bool, R::Error> {
    let key = DraftHistoricalRootAdoptionKeyV1::new(
        head.draft_id(),
        head.session_id(),
        transition.operation_id(),
    );
    let Some(settlement) = reader.point::<DraftHistoricalRootAdoptionsFamily>(&key)? else {
        return Ok(false);
    };
    if !settlement.is_locally_valid()
        || !reader.authenticate_frontier(settlement.source_history())?
    {
        return Ok(false);
    }
    historical::references_are_exact(reader, head, frontier, transition, &settlement)
}
