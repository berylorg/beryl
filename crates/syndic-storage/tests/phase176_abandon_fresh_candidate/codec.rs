use crate::{shared::*, support::*};

use syndic_storage::{
    DraftEditorCandidateSessionAbandonFreshOutcomeV1, DraftEditorCandidateSessionDisposeOutcomeV1,
    DraftEditorCandidateSessionDisposeRequestV1, DraftRootHistoryPairV1,
    test_candidate_disposal_receipt_codec_accepts,
};

#[test]
fn tag_three_accepts_only_clean_disposal_or_exact_fresh_abandonment() {
    let (_home, store, storage, thread) = fixture("abandon-codec", 170, 65_536);
    let selected = current(storage, &store, thread);
    let fresh_before = open_session(storage, &store, &selected, 172, 173);
    let fresh_request = abandon_request(&fresh_before, 174);
    let fresh_prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, fresh_request)
        .unwrap();
    let fresh_outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            fresh_prepared.clone(),
        ),
    );
    let fresh_after = match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session(
            &store,
            &fresh_prepared,
            fresh_outcome,
        )
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(head) => head,
        other => panic!("fresh abandonment fixture failed: {other:?}"),
    };
    let replay = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, fresh_request)
        .unwrap();
    let replay_outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            replay.clone(),
        ),
    );
    let fresh_receipt = match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &replay, replay_outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(receipt) => receipt,
        other => panic!("fresh abandonment receipt fixture failed: {other:?}"),
    };
    assert!(test_candidate_disposal_receipt_codec_accepts(
        fresh_request,
        fresh_before.clone(),
        fresh_after.clone(),
        fresh_receipt.frontier().clone(),
    ));

    let other_thread = create_thread(storage, &store, 180, 65_536);
    let other_selected = current(storage, &store, other_thread);
    let opened = open_session(storage, &store, &other_selected, 182, 183);
    let transaction = transaction(storage, &store, &opened, 184, "x", point(1));
    build(storage, &store, &transaction);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    let dirty = match settled(storage, &store, &transaction).closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => adoption.adopted_session().clone(),
        other => panic!("clean disposal edit fixture failed: {other:?}"),
    };
    publish_candidate(storage, &store, &other_selected, &dirty, 185);
    let clean_before = head(storage, &store, &opened);
    let clean_request = abandon_request(&clean_before, 186);
    let clean_prepared = storage
        .prepare_dispose_draft_editor_candidate_session(&store, clean_request)
        .unwrap();
    let clean_outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            clean_prepared.clone(),
        ),
    );
    let clean_after = match storage
        .reconcile_draft_editor_candidate_session_disposal(&store, &clean_prepared, clean_outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(head) => head,
        other => panic!("clean disposal fixture failed: {other:?}"),
    };
    let clean_replay = storage
        .prepare_dispose_draft_editor_candidate_session(&store, clean_request)
        .unwrap();
    let clean_replay_outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            clean_replay.clone(),
        ),
    );
    let clean_receipt = match storage
        .reconcile_draft_editor_candidate_session_disposal(
            &store,
            &clean_replay,
            clean_replay_outcome,
        )
        .unwrap()
    {
        DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(receipt) => receipt,
        other => panic!("clean disposal receipt fixture failed: {other:?}"),
    };
    assert!(test_candidate_disposal_receipt_codec_accepts(
        clean_request,
        clean_before.clone(),
        clean_after.clone(),
        clean_receipt.frontier().clone(),
    ));

    let wrong_operation = DraftPieceOperationIdV1::from_bytes([187; 16]);
    let unauthorized_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        fresh_request.draft_id(),
        fresh_request.session_id(),
        wrong_operation,
        fresh_request.expected_session_generation(),
        fresh_request.expected_pair(),
    );
    assert!(!test_candidate_disposal_receipt_codec_accepts(
        unauthorized_request,
        fresh_before.clone(),
        fresh_after.clone(),
        fresh_receipt.frontier().clone(),
    ));
    assert!(!test_candidate_disposal_receipt_codec_accepts(
        fresh_request,
        fresh_before.clone(),
        clean_after,
        fresh_receipt.frontier().clone(),
    ));
    assert!(!test_candidate_disposal_receipt_codec_accepts(
        fresh_request,
        fresh_before.clone(),
        fresh_after.clone(),
        clean_receipt.frontier().clone(),
    ));
    let stale_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        fresh_request.draft_id(),
        fresh_request.session_id(),
        fresh_request.operation_id(),
        fresh_request.expected_session_generation() + 1,
        DraftRootHistoryPairV1::new(fresh_before.newest_root(), fresh_before.newest_history()),
    );
    assert!(!test_candidate_disposal_receipt_codec_accepts(
        stale_request,
        fresh_before,
        fresh_after,
        fresh_receipt.frontier().clone(),
    ));
}
