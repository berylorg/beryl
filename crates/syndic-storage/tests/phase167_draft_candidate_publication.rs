use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidatePublicationCommandErrorV1, DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationOutcomeV1, DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidateSessionDisposeOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionOpenOutcomeV1,
    DraftEditorCandidateSessionOpenRequestV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftEditorCandidateSessionRecordKeyV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceOperationIdV1, DraftPieceOperationStatusV1, DraftPieceOperationVerificationV1,
    DraftPieceReplacementV1, DraftPieceSettlementKeyV1, DraftPieceV1, DraftRootHistoryPairV1,
    PreparedDraftPieceEditV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_fragment_chain_v1,
    test_faults::{
        DraftCandidatePublicationFault, delete_draft_edit_history_frontier,
        delete_draft_piece_terminal_build, inject_draft_candidate_publication_fault,
    },
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase167-{name}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Transaction {
    prepared: PreparedDraftPieceEditV1,
    fragments: Vec<DraftPieceBuildFragmentV1>,
}

#[test]
fn historical_replay_authenticates_the_current_session_fixed_point() {
    let (_home, store, storage, thread) = fixture("later-publication-closure", 130);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 131, 132);
    let first_edit = transaction(storage, &store, &session, 133, point(0), point(1), "a");
    settle(storage, &store, &first_edit);
    let first = adopted_head(storage, &store, &first_edit);
    let first_request = publication_request(&durable, &first, 134, 2);
    let first_prepared = storage
        .prepare_draft_editor_candidate_publication(&store, first_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(
            storage.revision(&store).unwrap(),
            first_prepared.clone(),
        ),
    );
    storage
        .reconcile_draft_editor_candidate_publication(&store, &first_prepared, outcome)
        .unwrap();

    let after_first = active_or_disposed(storage, &store, first.draft_id(), first.session_id());
    let second_edit = transaction(storage, &store, &after_first, 135, point(1), point(2), "b");
    settle(storage, &store, &second_edit);
    let second = adopted_head(storage, &store, &second_edit);
    let after_first_current = current(storage, &store, thread);
    let second_request = publication_request(&after_first_current, &second, 136, 4);
    let second_prepared = storage
        .prepare_draft_editor_candidate_publication(&store, second_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(
            storage.revision(&store).unwrap(),
            second_prepared.clone(),
        ),
    );
    storage
        .reconcile_draft_editor_candidate_publication(&store, &second_prepared, outcome)
        .unwrap();
    let _ = execute(
        &store,
        inject_draft_candidate_publication_fault(
            &store,
            storage,
            DraftCandidatePublicationFault::DeleteSessionRecord(
                DraftEditorCandidateSessionRecordKeyV1::publication_receipt(
                    second.draft_id(),
                    second.session_id(),
                    second_request.operation_id(),
                ),
            ),
        ),
    );
    assert!(
        storage
            .prepare_draft_editor_candidate_publication(&store, first_request)
            .is_err()
    );

    let (_home, store, storage, thread) = fixture("later-adoption-closure", 140);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 141, 142);
    let first_edit = transaction(storage, &store, &session, 143, point(0), point(1), "a");
    settle(storage, &store, &first_edit);
    let first = adopted_head(storage, &store, &first_edit);
    let first_request = publication_request(&durable, &first, 144, 2);
    let first_prepared = storage
        .prepare_draft_editor_candidate_publication(&store, first_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(
            storage.revision(&store).unwrap(),
            first_prepared.clone(),
        ),
    );
    storage
        .reconcile_draft_editor_candidate_publication(&store, &first_prepared, outcome)
        .unwrap();
    let after_first = active_or_disposed(storage, &store, first.draft_id(), first.session_id());
    let dirty_edit = transaction(storage, &store, &after_first, 145, point(1), point(2), "b");
    settle(storage, &store, &dirty_edit);
    let _ = adopted_head(storage, &store, &dirty_edit);
    let key = DraftPieceSettlementKeyV1::new(
        dirty_edit.prepared.header().draft_id(),
        dirty_edit.prepared.header().session_id(),
        dirty_edit.prepared.header().operation_id(),
    );
    let _ = execute(
        &store,
        delete_draft_piece_terminal_build(&store, storage, key),
    );
    assert!(
        storage
            .prepare_draft_editor_candidate_publication(&store, first_request)
            .is_err()
    );
}

#[test]
fn stale_disposal_expectations_do_not_mutate_and_identity_collision_is_typed() {
    let (_home, store, storage, thread) = fixture("stale-disposal", 1);
    let (_publication, head) = publish_one(storage, &store, thread, 2);
    let pair = DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history());
    for request in [
        DraftEditorCandidateSessionDisposeRequestV1::new(
            head.draft_id(),
            head.session_id(),
            DraftPieceOperationIdV1::from_bytes([4; 16]),
            head.session_generation() + 1,
            pair,
        ),
        DraftEditorCandidateSessionDisposeRequestV1::new(
            head.draft_id(),
            head.session_id(),
            DraftPieceOperationIdV1::from_bytes([5; 16]),
            head.session_generation(),
            DraftRootHistoryPairV1::new(head.durable_base_root(), head.durable_base_history()),
        ),
    ] {
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
        assert!(matches!(
            storage
                .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
                .unwrap(),
            DraftEditorCandidateSessionDisposeOutcomeV1::DirtyConflict(_)
        ));
    }
    let operation = DraftPieceOperationIdV1::from_bytes([6; 16]);
    let exact = DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        operation,
        head.session_generation(),
        pair,
    );
    let prepared = storage
        .prepare_dispose_draft_editor_candidate_session(&store, exact)
        .unwrap();
    let outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
    ));
    let collision_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        operation,
        head.session_generation() + 1,
        pair,
    );
    let collision = storage
        .prepare_dispose_draft_editor_candidate_session(&store, collision_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            collision.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &collision, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::OccupiedIdentityCollision(_)
    ));
}

#[test]
fn publication_receipt_and_snapshot_partial_occupancy_fail_closed() {
    for (name, case, seed) in [
        ("pub-receipt-cut", 0_u8, 70_u8),
        ("pub-frontier-cut", 1, 80),
        ("pub-receipt-corrupt", 2, 90),
    ] {
        let (_home, store, storage, thread) = fixture(name, seed);
        let (request, head) = publish_one(storage, &store, thread, 90);
        let receipt_key = DraftEditorCandidateSessionRecordKeyV1::publication_receipt(
            head.draft_id(),
            head.session_id(),
            request.operation_id(),
        );
        let contribution = match case {
            0 => inject_draft_candidate_publication_fault(
                &store,
                storage,
                DraftCandidatePublicationFault::DeleteSessionRecord(receipt_key),
            ),
            1 => {
                delete_draft_edit_history_frontier(&store, storage, head.published_history().key())
            }
            _ => inject_draft_candidate_publication_fault(
                &store,
                storage,
                DraftCandidatePublicationFault::OccupyReceiptWithHead {
                    receipt_key,
                    draft_id: head.draft_id(),
                    session_id: head.session_id(),
                },
            ),
        };
        let _ = execute(&store, contribution);
        assert!(matches!(
            storage.draft_editor_candidate_session(&store, head.draft_id(), head.session_id()),
            Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure) | Err(_)
        ));
        assert!(
            storage
                .prepare_draft_editor_candidate_publication(&store, request)
                .is_err()
        );
    }
}

#[test]
fn disposal_receipt_head_only_and_impossible_pointer_fail_closed() {
    for (case, seed) in [(0_u8, 100_u8), (1, 110), (2, 120)] {
        let (_home, store, storage, thread) = fixture("dispose-corruption", seed);
        let (_publication, head) = publish_one(storage, &store, thread, seed + 1);
        let operation = DraftPieceOperationIdV1::from_bytes([seed + 9; 16]);
        let request = DraftEditorCandidateSessionDisposeRequestV1::new(
            head.draft_id(),
            head.session_id(),
            operation,
            head.session_generation(),
            DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
        );
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
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
            .unwrap();
        let receipt_key = DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
            head.draft_id(),
            head.session_id(),
            operation,
        );
        let fault = match case {
            0 => DraftCandidatePublicationFault::DeleteSessionRecord(receipt_key),
            1 => DraftCandidatePublicationFault::OccupyReceiptWithHead {
                receipt_key,
                draft_id: head.draft_id(),
                session_id: head.session_id(),
            },
            _ => DraftCandidatePublicationFault::RetargetDisposedHead {
                draft_id: head.draft_id(),
                session_id: head.session_id(),
                operation_id: DraftPieceOperationIdV1::from_bytes([seed + 3; 16]),
            },
        };
        let _ = execute(
            &store,
            inject_draft_candidate_publication_fault(&store, storage, fault),
        );
        assert!(matches!(
            storage
                .draft_editor_candidate_session(&store, head.draft_id(), head.session_id())
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
        ));
        assert!(
            storage
                .prepare_dispose_draft_editor_candidate_session(&store, request)
                .is_err()
        );
    }
}

#[test]
fn publication_replay_restart_and_clean_disposal_are_exact_and_nondeleting() {
    let (home, store, storage, thread) = fixture("publish-dispose", 10);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 20, 21);
    let edit = transaction(storage, &store, &session, 22, point(0), point(3), "abc");
    settle(storage, &store, &edit);
    let adopted = adopted_head(storage, &store, &edit);
    let request = publication_request(&durable, &adopted, 23, 2);
    let prepared = storage
        .prepare_draft_editor_candidate_publication(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    let published = storage
        .reconcile_draft_editor_candidate_publication(&store, &prepared, outcome)
        .unwrap();
    assert!(matches!(
        published,
        DraftEditorCandidatePublicationOutcomeV1::Published(_, pair)
            if pair.root() == request.candidate().root()
                && pair.history().candidate_generation() == request.candidate_generation()
                && pair.history() != request.candidate().history()
    ));
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        adopted.newest_root()
    );

    let replay = storage
        .prepare_draft_editor_candidate_publication(&store, request)
        .unwrap();
    let replay_outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), replay.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &replay, replay_outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::ExactReplay(_)
    ));

    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let head = active_or_disposed(storage, &store, adopted.draft_id(), adopted.session_id());
    let dispose_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([24; 16]),
        head.session_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
    );
    let dispose = storage
        .prepare_dispose_draft_editor_candidate_session(&store, dispose_request)
        .unwrap();
    let disposal_outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            dispose.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &dispose, disposal_outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
    ));
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        adopted.newest_root()
    );
    let replay = storage
        .prepare_dispose_draft_editor_candidate_session(&store, dispose_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            replay.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &replay, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_)
    ));
}

#[test]
fn publishing_captured_generation_preserves_a_newer_dirty_candidate() {
    let (_home, store, storage, thread) = fixture("newer-dirty", 30);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 31, 32);
    let first_edit = transaction(storage, &store, &session, 33, point(0), point(3), "abc");
    settle(storage, &store, &first_edit);
    let first = adopted_head(storage, &store, &first_edit);
    let request = publication_request(&durable, &first, 35, 3);
    let prepared = storage
        .prepare_draft_editor_candidate_publication(&store, request)
        .unwrap();
    let second_edit = transaction(storage, &store, &first, 34, point(3), point(4), "d");
    settle(storage, &store, &second_edit);
    let second = adopted_head(storage, &store, &second_edit);
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let head = active_or_disposed(storage, &store, second.draft_id(), second.session_id());
    assert_eq!(
        head.published_candidate_generation(),
        first.newest_candidate_generation()
    );
    assert_eq!(
        head.newest_candidate_generation(),
        second.newest_candidate_generation()
    );
    assert_eq!(head.newest_root(), second.newest_root());
}

#[test]
fn editing_after_publication_forks_from_the_immutable_snapshot() {
    let (_home, store, storage, thread) = fixture("edit-after-publish", 40);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 41, 42);
    let first_edit = transaction(storage, &store, &session, 43, point(0), point(1), "x");
    settle(storage, &store, &first_edit);
    let first = adopted_head(storage, &store, &first_edit);
    let request = publication_request(&durable, &first, 44, 2);
    let publication = storage
        .prepare_draft_editor_candidate_publication(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage
            .publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &publication, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let published_head = active_or_disposed(storage, &store, first.draft_id(), first.session_id());
    let second_edit = transaction(
        storage,
        &store,
        &published_head,
        45,
        point(1),
        point(2),
        "y",
    );
    settle(storage, &store, &second_edit);
    let second = adopted_head(storage, &store, &second_edit);
    assert_eq!(
        second.published_history(),
        published_head.published_history()
    );
    assert_ne!(second.newest_history(), published_head.published_history());
    let replay = storage
        .prepare_draft_editor_candidate_publication(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), replay.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &replay, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::ExactReplay(_)
    ));
}

#[test]
fn dirty_disposal_and_durable_base_conflict_are_typed_without_mutation() {
    let (_home, store, storage, thread) = fixture("conflicts", 50);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 51, 52);
    let edit = transaction(storage, &store, &session, 53, point(0), point(1), "x");
    settle(storage, &store, &edit);
    let dirty = adopted_head(storage, &store, &edit);
    let request = DraftEditorCandidateSessionDisposeRequestV1::new(
        dirty.draft_id(),
        dirty.session_id(),
        DraftPieceOperationIdV1::from_bytes([54; 16]),
        dirty.session_generation(),
        DraftRootHistoryPairV1::new(dirty.newest_root(), dirty.newest_history()),
    );
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
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::DirtyConflict(_)
    ));

    let stale_selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision().checked_next().unwrap(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let publish = DraftEditorCandidatePublicationRequestV1::new(
        stale_selector,
        dirty.session_id(),
        DraftPieceOperationIdV1::from_bytes([55; 16]),
        dirty.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(dirty.newest_root(), dirty.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(4),
    );
    let prepared = storage
        .prepare_draft_editor_candidate_publication(&store, publish)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::DurableBaseConflict(_)
    ));
}

#[test]
fn active_custody_collisions_supersession_and_already_disposed_are_typed() {
    let (_home, store, storage, thread) = fixture("typed-outcomes", 60);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 61, 62);
    let edit = transaction(storage, &store, &session, 63, point(0), point(1), "x");
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let active_request = publication_request(&durable, &session, 64, 2);
    assert!(matches!(
        storage.prepare_draft_editor_candidate_publication(&store, active_request),
        Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation)
    ));
    settle_after_begin(storage, &store, &edit);
    let adopted = adopted_head(storage, &store, &edit);
    let request = publication_request(&durable, &adopted, 65, 3);
    let prepared = storage
        .prepare_draft_editor_candidate_publication(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));

    let collision_request = DraftEditorCandidatePublicationRequestV1::new(
        request.selector(),
        request.session_id(),
        request.operation_id(),
        request.candidate_generation(),
        request.candidate(),
        request.evidence(),
        SyndicTimestamp::from_unix_millis(4),
    );
    let collision = storage
        .prepare_draft_editor_candidate_publication(&store, collision_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage
            .publish_draft_editor_candidate(storage.revision(&store).unwrap(), collision.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &collision, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::OccupiedIdentityCollision(_)
    ));

    let superseded_request = DraftEditorCandidatePublicationRequestV1::new(
        request.selector(),
        request.session_id(),
        DraftPieceOperationIdV1::from_bytes([66; 16]),
        request.candidate_generation(),
        request.candidate(),
        request.evidence(),
        request.published_at(),
    );
    let superseded = storage
        .prepare_draft_editor_candidate_publication(&store, superseded_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage
            .publish_draft_editor_candidate(storage.revision(&store).unwrap(), superseded.clone()),
    );
    assert!(matches!(
        storage.reconcile_draft_editor_candidate_publication(&store, &superseded, outcome).unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Superseded(generation, _)
            if generation == request.candidate_generation()
    ));

    let head = active_or_disposed(storage, &store, adopted.draft_id(), adopted.session_id());
    let dispose_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([67; 16]),
        head.session_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
    );
    let dispose = storage
        .prepare_dispose_draft_editor_candidate_session(&store, dispose_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            dispose.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &dispose, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
    ));
    let already_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([68; 16]),
        head.session_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
    );
    let already = storage
        .prepare_dispose_draft_editor_candidate_session(&store, already_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            already.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &already, outcome)
            .unwrap(),
        DraftEditorCandidateSessionDisposeOutcomeV1::AlreadyDisposed(_)
    ));
}

#[test]
fn publication_and_disposal_reconcile_every_atomic_command_cut() {
    for (name, seed, fault, committed_at_cut) in [
        ("before-commit", 70, FaultPoint::BeforeCommit, false),
        (
            "after-commit",
            80,
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        ("after-persist", 90, FaultPoint::AfterPersist, true),
        (
            "before-verification",
            100,
            FaultPoint::BeforeVerification,
            true,
        ),
    ] {
        let (home, store, storage, faults, thread) = fault_fixture(name, seed);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, seed + 2, seed + 3);
        let edit = transaction(storage, &store, &session, seed + 4, point(0), point(1), "x");
        settle(storage, &store, &edit);
        let adopted = adopted_head(storage, &store, &edit);
        let request = publication_request(&durable, &adopted, seed + 5, 2);
        let prepared = storage
            .prepare_draft_editor_candidate_publication(&store, request)
            .unwrap();
        faults.fail_next(fault);
        let outcome = execute(
            &store,
            storage.publish_draft_editor_candidate(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        let (store, storage) = recover_if_failed(store, storage);
        let reconciled =
            storage.reconcile_draft_editor_candidate_publication(&store, &prepared, outcome);
        if committed_at_cut {
            assert!(matches!(
                reconciled.unwrap(),
                DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
            ));
        } else {
            assert!(matches!(
                reconciled,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
        }
        if !committed_at_cut {
            let retry = execute(
                &store,
                storage.publish_draft_editor_candidate(
                    storage.revision(&store).unwrap(),
                    prepared.clone(),
                ),
            );
            assert!(matches!(
                storage
                    .reconcile_draft_editor_candidate_publication(&store, &prepared, retry)
                    .unwrap(),
                DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
            ));
        }
        let head = active_or_disposed(storage, &store, adopted.draft_id(), adopted.session_id());
        let dispose_request = DraftEditorCandidateSessionDisposeRequestV1::new(
            head.draft_id(),
            head.session_id(),
            DraftPieceOperationIdV1::from_bytes([seed + 6; 16]),
            head.session_generation(),
            DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
        );
        let dispose = storage
            .prepare_dispose_draft_editor_candidate_session(&store, dispose_request)
            .unwrap();
        faults.fail_next(fault);
        let outcome = execute(
            &store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                dispose.clone(),
            ),
        );
        let (store, storage) = recover_if_failed(store, storage);
        let reconciled =
            storage.reconcile_draft_editor_candidate_session_disposal(&store, &dispose, outcome);
        if committed_at_cut {
            assert!(matches!(
                reconciled.unwrap(),
                DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
            ));
        } else {
            assert!(matches!(
                reconciled,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
        }
        drop(store);
        drop(home);
    }
}

fn publish_one(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> (
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidateSessionV1,
) {
    let durable = current(storage, store, thread);
    let session = open_session(storage, store, &durable, seed, seed.wrapping_add(1));
    let edit = transaction(
        storage,
        store,
        &session,
        seed.wrapping_add(2),
        point(0),
        point(1),
        "x",
    );
    settle(storage, store, &edit);
    let adopted = adopted_head(storage, store, &edit);
    let request = publication_request(&durable, &adopted, seed.wrapping_add(3), 2);
    let prepared = storage
        .prepare_draft_editor_candidate_publication(store, request)
        .unwrap();
    let outcome = execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    storage
        .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
        .unwrap();
    (
        request,
        active_or_disposed(storage, store, adopted.draft_id(), adopted.session_id()),
    )
}

fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase167",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

fn fault_fixture(
    name: &str,
    seed: u8,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    FaultController,
    SyndicThreadId,
) {
    let home = TestHome::new(name);
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase167",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, faults, thread)
}

fn recover_if_failed(store: HomeStore, storage: SyndicStorage) -> (HomeStore, SyndicStorage) {
    if store.health().state() == HomeHealthState::Failed {
        let recovery = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        (recovery.publish(), storage)
    } else {
        (store, storage)
    }
}

fn current(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> syndic_storage::SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

fn selector(current: &syndic_storage::SyndicCurrentDraft) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    )
}

fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(current),
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
    );
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(store, request)
        .unwrap();
    let outcome = execute(
        store,
        storage.open_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            prepared.clone(),
        ),
    );
    match storage
        .reconcile_draft_editor_candidate_session_open(store, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => head,
        other => panic!("session did not open: {other:?}"),
    }
}

fn transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    before: DraftCompositePositionV1,
    after: DraftCompositePositionV1,
    text: &str,
) -> Transaction {
    let replacements = vec![DraftPieceReplacementV1::new(
        before,
        before,
        vec![DraftPieceV1::Text(text.to_owned())],
    )];
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        before,
        before,
        after,
        after,
        1,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, session)
        .unwrap();
    let fragment = storage
        .prepare_draft_piece_fragment(
            &prepared,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            replacements.into_iter().next().unwrap(),
        )
        .unwrap();
    Transaction {
        prepared,
        fragments: vec![fragment],
    }
}

fn settle(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    settle_after_begin(storage, store, transaction);
}

fn settle_after_begin(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    for fragment in &transaction.fragments {
        committed(execute(
            store,
            storage.stage_draft_piece_fragment(
                storage.revision(store).unwrap(),
                transaction.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            transaction.prepared.header().draft_id(),
            transaction.prepared.header().session_id(),
            transaction.prepared.header().operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
}

fn adopted_head(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_piece_operation_status_page(store, &transaction.prepared, 1, &transaction.fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Settled(
            settlement,
        )) => match settlement.closure() {
            syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
                adoption.adopted_session().clone()
            }
            _ => panic!("edit was not adopted"),
        },
        other => panic!("edit did not settle: {other:?}"),
    }
}

fn publication_request(
    current: &syndic_storage::SyndicCurrentDraft,
    head: &DraftEditorCandidateSessionV1,
    operation: u8,
    at: u64,
) -> DraftEditorCandidatePublicationRequestV1 {
    DraftEditorCandidatePublicationRequestV1::new(
        selector(current),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        head.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(at),
    )
}

fn active_or_disposed(
    storage: SyndicStorage,
    store: &HomeStore,
    draft: SyndicDraftId,
    session: DraftEditorCandidateSessionIdV1,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(store, draft, session)
        .unwrap()
    {
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(head)
        | syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
        other => panic!("session unavailable: {other:?}"),
    }
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "command was not cleanly committed: {outcome:?}"
    );
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}
