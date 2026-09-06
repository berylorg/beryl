use super::{edit_support::commit_edit, publication_support::*, support::*};
use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1, DraftEditorCandidateSessionDisposeOutcomeV1,
    DraftEditorCandidateSessionDisposeRequestV1, DraftRootHistoryPairV1,
};

#[test]
fn ordinary_opening_disposal_preserves_the_selector_and_replays_the_normalized_head() {
    for populated in [false, true] {
        let (home, store, storage, thread) = fixture("disposal", 90, 65_536);
        let initial = current(&storage, &store, thread);
        if populated {
            let opened = open_session(&storage, &store, &initial, 92, 93);
            let edit = commit_edit(&storage, &store, &opened, 94, "saved before reopening");
            publish_candidate(&storage, &store, &initial, edit.adopted_session(), 95);
        }
        let durable = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &durable, 96, 97);
        let request = disposal_request(&opened, 98);
        let prepared = storage
            .prepare_dispose_draft_editor_candidate_session(&store, request)
            .unwrap();
        let outcome = execute(
            &store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        let terminal = match storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
            .unwrap()
        {
            DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(head) => head,
            other => panic!("ordinary opening disposal did not commit: {other:?}"),
        };
        assert_eq!(terminal.newest_root(), opened.newest_root());
        assert_eq!(terminal.newest_root(), terminal.published_root());
        assert_eq!(terminal.newest_history(), terminal.published_history());
        assert_eq!(terminal.published_history(), durable.draft().history());
        assert_eq!(
            terminal.session_generation(),
            opened.session_generation() + 1
        );
        assert_eq!(current(&storage, &store, thread), durable);
        drop(store);
        let mut store = open(&home);
        let storage = SyndicStorage::register(&mut store).unwrap();
        assert_eq!(head(&storage, &store, &opened), terminal);
        assert_receipt_replay(&storage, &store, request, &opened, &terminal);
        assert_eq!(current(&storage, &store, thread), durable);
    }
}

#[test]
fn normalized_opening_disposal_and_its_receipt_survive_atomic_command_cuts() {
    for (fault, committed_at_cut) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterCommitBeforePersist, true),
        (FaultPoint::AfterPersist, true),
        (FaultPoint::BeforeVerification, true),
    ] {
        let (_home, store, storage, faults, thread) = fault_fixture("disposal-cut", 110, 65_536);
        let durable = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &durable, 112, 113);
        let prepared = storage
            .prepare_dispose_draft_editor_candidate_session(&store, disposal_request(&opened, 114))
            .unwrap();
        faults.fail_next(fault);
        let outcome = execute(
            &store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        let (store, storage) = recover_if_failed(store, storage);
        let reconciled =
            storage.reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome);
        if committed_at_cut {
            let terminal = match reconciled.unwrap() {
                DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(head) => head,
                other => panic!("committed disposal cut was not exact: {other:?}"),
            };
            assert_eq!(terminal.newest_root(), terminal.published_root());
            assert_eq!(terminal.newest_history(), terminal.published_history());
            assert_eq!(head(&storage, &store, &opened), terminal);
            assert_receipt_replay(
                &storage,
                &store,
                disposal_request(&opened, 114),
                &opened,
                &terminal,
            );
        } else {
            assert!(
                matches!(
                    reconciled,
                    Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
                ),
                "before-commit disposal result variant: {:?}",
                reconciled.as_ref().map(std::mem::discriminant)
            );
            assert_eq!(head(&storage, &store, &opened), opened);
        }
        assert_eq!(current(&storage, &store, thread), durable);
    }
}

#[test]
fn ordinary_opening_disposal_rejects_the_private_frontier_pair() {
    let (_home, store, storage, thread) = fixture("private-pair", 190, 65_536);
    let durable = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &durable, 192, 193);
    let request = DraftEditorCandidateSessionDisposeRequestV1::new(
        opened.draft_id(),
        opened.session_id(),
        DraftPieceOperationIdV1::from_bytes([194; 16]),
        opened.session_generation(),
        DraftRootHistoryPairV1::new(opened.newest_root(), opened.newest_history()),
    );
    assert_disposal_rejected(&storage, &store, request);
    assert_eq!(head(&storage, &store, &opened), opened);
    assert_eq!(current(&storage, &store, thread), durable);
}

#[test]
fn ordinary_opening_disposal_does_not_normalize_a_later_dirty_candidate() {
    let (_home, store, storage, thread) = fixture("dirty-disposal", 210, 65_536);
    let durable = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &durable, 212, 213);
    let edited = commit_edit(&storage, &store, &opened, 214, "unsaved successor");
    let dirty = edited.adopted_session();
    assert_disposal_rejected(&storage, &store, disposal_request(dirty, 215));
    assert_eq!(head(&storage, &store, dirty), *dirty);
    assert_eq!(current(&storage, &store, thread), durable);
}

fn assert_disposal_rejected(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: DraftEditorCandidateSessionDisposeRequestV1,
) {
    let before = store.home_revision().unwrap();
    if let Ok(prepared) = storage.prepare_dispose_draft_editor_candidate_session(store, request) {
        assert!(matches!(
            execute(
                store,
                storage.dispose_draft_editor_candidate_session(
                    storage.revision(store).unwrap(),
                    prepared
                )
            ),
            CommandOutcome::NotCommitted { .. }
        ));
    }
    assert_eq!(store.home_revision().unwrap(), before);
}

pub(super) fn assert_receipt_replay(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: DraftEditorCandidateSessionDisposeRequestV1,
    before_head: &DraftEditorCandidateSessionV1,
    after_head: &DraftEditorCandidateSessionV1,
) {
    let before = store.home_revision().unwrap();
    let replay = storage
        .prepare_dispose_draft_editor_candidate_session(store, request)
        .unwrap();
    let outcome = execute(
        store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            replay.clone(),
        ),
    );
    assert!(matches!(&outcome, CommandOutcome::NotCommitted { .. }));
    let replay = storage
        .reconcile_draft_editor_candidate_session_disposal(store, &replay, outcome)
        .unwrap();
    let DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(receipt) = replay else {
        panic!("disposal receipt did not replay exactly: {replay:?}");
    };
    assert_eq!(receipt.before_head(), before_head);
    assert_eq!(receipt.after_head(), after_head);
    assert_eq!(store.home_revision().unwrap(), before);
}

fn disposal_request(
    head: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftEditorCandidateSessionDisposeRequestV1 {
    DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        head.session_generation(),
        DraftRootHistoryPairV1::new(head.published_root(), head.published_history()),
    )
}
