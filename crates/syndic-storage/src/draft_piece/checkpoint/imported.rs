use super::*;
use crate::codec::{CanonicalItemsFamily, DraftByThreadFamily, DraftsFamily, ThreadsFamily};

pub(super) fn published_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, R::Error> {
    let Some(c) = receipt(reader, head.published_history())? else {
        return Ok(false);
    };
    let current_draft = reader.point::<DraftsFamily>(&head.draft_id())?;
    let reverse = reader.point::<DraftByThreadFamily>(&head.thread_id())?;
    let thread = reader.point::<ThreadsFamily>(&head.thread_id())?;
    let (Some(draft), Some(reverse), Some(thread)) = (current_draft, reverse, thread) else {
        return Ok(false);
    };
    let same_session = c.after_session.session_id() == head.session_id();
    Ok(c.baseline.reference() == head.published_history()
        && c.imported_root == head.published_root()
        && c.successor_selector.selector_revision() <= head.published_selector_revision()
        && c.after_session.published_candidate_generation()
            == head.published_candidate_generation()
        && draft.id() == head.draft_id()
        && draft.thread_id() == head.thread_id()
        && thread.current_draft_id() == draft.id()
        && reverse.thread_id() == thread.id()
        && reverse.draft_id() == draft.id()
        && reverse.draft_revision() == draft.revision()
        && reverse.thread_revision() == thread.revision()
        && draft.revision() == head.published_selector_revision()
        && draft.piece_root() == head.published_root()
        && draft.history() == head.published_history()
        && if same_session {
            same_opening(&c.after_session, head)
                && head.session_generation() >= c.after_session.session_generation()
        } else {
            head.durable_base_root() == c.imported_root
                && head.durable_base_history() == c.baseline.reference()
                && head.durable_base_selector_revision() <= head.published_selector_revision()
        })
}
pub(super) fn candidate_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    let Some(c) = receipt(reader, head.newest_history())? else {
        return Ok(false);
    };
    let mutable = reader
        .point::<DraftEditHistoryFrontiersFamily>(&c.after_mutable_history.reference().key())?;
    Ok(c.baseline.as_ref() == frontier
        && head.newest_candidate_generation() == c.after_session.newest_candidate_generation()
        && head.newest_root() == c.imported_root
        && head.newest_history() == c.baseline.reference()
        && head.published_history() == c.baseline.reference()
        && head.published_root() == c.imported_root
        && head.published_candidate_generation() == head.newest_candidate_generation()
        && head.dirty_generation() == c.after_session.dirty_generation()
        && same_opening(&c.after_session, head)
        && head.session_generation() >= c.after_session.session_generation()
        && mutable.as_ref() == Some(c.after_mutable_history.as_ref())
        && published_is_exact(reader, head)?)
}
pub(super) fn baseline_fork_is_exact<R: CheckpointReader>(
    reader: &R,
    baseline: &DraftEditHistoryFrontierV1,
    fork: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    let Some(c) = receipt(reader, baseline.reference())? else {
        return Ok(false);
    };
    let actual = reader.point::<DraftEditHistoryFrontiersFamily>(&fork.reference().key())?;
    Ok(c.baseline.as_ref() == baseline
        && c.after_mutable_history.as_ref() == fork
        && actual.as_ref() == Some(fork))
}

fn receipt<R: CheckpointReader>(
    reader: &R,
    reference: DraftEditHistoryFrontierReferenceV1,
) -> Result<Option<Box<DraftSealedImportSelectionV1>>, R::Error> {
    let DraftEditHistoryFrontierKeyV1::SealedImportBaseline {
        draft_id,
        session_id,
        operation_id,
    } = reference.key()
    else {
        return Ok(None);
    };
    let key = DraftPieceSettlementKeyV1::new(draft_id, session_id, operation_id);
    let stored = reader.point::<DraftPieceSettlementsFamily>(&key)?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let DraftPieceSettlementClosureV1::SealedImport(c) = stored.closure() else {
        return Ok(None);
    };
    if !settlement_closure_is_exact(&stored) || c.baseline.reference() != reference {
        return Ok(None);
    }
    let terminal = reader.point::<DraftPieceBuildsFamily>(&key)?;
    let Some(terminal) = terminal else {
        return Ok(None);
    };
    if !settlement_terminal_build_is_exact(&stored, Some(&terminal)) {
        return Ok(None);
    }
    let progress =
        reader.point::<DraftPieceBuildProgressFamily>(&stored.terminal_receipt().key())?;
    let Some(progress) = progress else {
        return Ok(None);
    };
    if !progress_receipt_matches_build(&progress, &terminal)
        || !reader.authenticate_progress(&progress)?
    {
        return Ok(None);
    }
    let Some(next) = progress.key().transition_ordinal().checked_add(1) else {
        return Ok(None);
    };
    if reader
        .point::<DraftPieceBuildProgressFamily>(&DraftPieceBuildProgressReceiptKeyV1::new(
            draft_id,
            session_id,
            operation_id,
            next,
        ))?
        .is_some()
    {
        return Ok(None);
    }
    let root = reader.point::<DraftPieceRootsFamily>(&c.imported_root.key())?;
    let baseline = reader.point::<DraftEditHistoryFrontiersFamily>(&reference.key())?;
    let item = reader.point::<CanonicalItemsFamily>(&c.basis.item_id())?;
    if root
        .as_ref()
        .is_none_or(|r| r.reference() != c.imported_root)
        || baseline.as_ref() != Some(c.baseline.as_ref())
        || !reader.authenticate_frontier(&c.baseline)?
        || item.as_ref().is_none_or(|i| {
            i.id() != c.basis.item_id()
                || i.revision() != c.basis.item_revision()
                || i.turn_id() != c.basis.intent().target_turn_id()
                || i.presentation()
                    != &crate::CanonicalItemPresentation::user_input(
                        c.basis.content(),
                        c.basis.asset_reference_set(),
                    )
        })
    {
        return Ok(None);
    }
    Ok(Some(Box::new(c.clone())))
}
fn same_opening(a: &DraftEditorCandidateSessionV1, b: &DraftEditorCandidateSessionV1) -> bool {
    a.thread_id() == b.thread_id()
        && a.draft_id() == b.draft_id()
        && a.session_id() == b.session_id()
        && a.open_operation_id() == b.open_operation_id()
        && a.durable_base_root() == b.durable_base_root()
        && a.durable_base_history() == b.durable_base_history()
        && a.durable_base_selector_revision() == b.durable_base_selector_revision()
}
