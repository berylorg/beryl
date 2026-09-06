use super::support::*;

pub(super) fn commit_edit(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    text: &str,
) -> syndic_storage::DraftPieceCommittedAdoptionV1 {
    let edit = transaction(
        storage,
        store,
        session,
        operation,
        text,
        point(text.len() as u64),
    );
    build(storage, store, &edit);
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(storage, store, &edit);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("ordinary edit did not commit: {:?}", settlement.outcome());
    };
    adoption.clone()
}
