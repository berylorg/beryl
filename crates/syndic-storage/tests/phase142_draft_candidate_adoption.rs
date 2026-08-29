use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceErrorReasonV1, DraftPieceMarkerAtV1, DraftPieceMarkerV1, DraftPieceOperationIdV1,
    DraftPieceOperationStatusV1, DraftPieceOperationVerificationV1, DraftPiecePrepareErrorV1,
    DraftPieceRejectedReasonV1, DraftPieceReplacementV1, DraftPieceSettlementOutcomeV1,
    DraftPieceTextDemandV1, DraftPieceTransactionOutcomeV1, DraftPieceV1, PreparedDraftPieceEditV1,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    DraftPieceBuildFragmentKeyV1, DraftPieceBuildFrontierV1,
    test_faults::{
        DraftEditorCandidateOpenReceiptCorruption, DraftPieceBuildCorruption,
        DraftPieceCandidateRootCollision, DraftPieceDescendantCorruption,
        DraftPieceDescendantTarget, DraftPieceFragmentCorruption, DraftPieceImmutableDeletion,
        DraftPieceProgressReceiptCorruption, delete_draft_piece_immutable_record,
        delete_draft_piece_terminal_build, draft_piece_fragment_is_stored_exactly,
        draft_piece_fragment_zero_ordinal_codec_rejections,
        inject_draft_editor_candidate_open_receipt_corruption,
        inject_draft_editor_candidate_session_published_beyond_newest,
        inject_draft_piece_build_corruption, inject_draft_piece_candidate_root_collision,
        inject_draft_piece_coordinated_stage_target_replacement,
        inject_draft_piece_custody_endpoint_corruption, inject_draft_piece_descendant_corruption,
        inject_draft_piece_fragment_ahead, inject_draft_piece_fragment_corruption,
        inject_draft_piece_occupied_stage_target, inject_draft_piece_progress_receipt_corruption,
        inject_draft_piece_session_generation_inflation,
        inject_draft_piece_settlement_closure_corruption,
    },
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase142-{name}-{}-{}",
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

#[derive(Clone, Copy)]
struct FixturePositions {
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
}

thread_local! {
    static FIXTURE_POSITIONS: std::cell::RefCell<Vec<(
        syndic_storage::DraftEditHistoryFrontierReferenceV1,
        FixturePositions,
    )>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn fixture_positions(session: &DraftEditorCandidateSessionV1) -> FixturePositions {
    if session.newest_history().frontier_revision() == 0 {
        return FixturePositions {
            caret: point(0),
            selection: point(0),
        };
    }
    FIXTURE_POSITIONS.with(|positions| {
        positions
            .borrow()
            .iter()
            .rev()
            .find(|(history, _)| *history == session.newest_history())
            .map(|(_, positions)| *positions)
            .expect("fixture must remember the exact positions of an adopted candidate head")
    })
}

#[test]
fn created_thread_has_an_independent_empty_image_label_authority_head() {
    let (_home, store, storage, thread) = fixture("label-authority-head", 5);
    let head = storage
        .image_label_authority_head(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .expect("created thread has a label authority head");
    assert_eq!(head.thread_id(), thread);
    assert_eq!(head.revision(), 1);
    assert_eq!(head.inherited(), syndic_storage::ImageLabelFrontier::EMPTY);
    assert_eq!(head.permanent(), syndic_storage::ImageLabelFrontier::EMPTY);
    assert!(head.is_exact());
}

fn remember_fixture_positions(
    session: &DraftEditorCandidateSessionV1,
    positions: FixturePositions,
) {
    FIXTURE_POSITIONS.with(|known| {
        known
            .borrow_mut()
            .push((session.newest_history(), positions));
    });
}

#[cfg(feature = "test-faults")]
#[test]
fn fragment_ordinals_are_one_based_in_codec_and_durable_storage() {
    let (_home, store, storage, thread) = fixture("one-based-fragment-ordinals", 6);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 7, 8);
    let edit = transaction(
        &storage,
        &store,
        &session,
        9,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("first".to_owned())],
            ),
            DraftPieceReplacementV1::continuation(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("second".to_owned())],
            ),
        ],
        point(11),
    );
    assert_eq!(edit.fragments[0].key().ordinal(), 1);
    assert_eq!(edit.fragments[1].key().ordinal(), 2);
    assert_eq!(
        draft_piece_fragment_zero_ordinal_codec_rejections(&edit.fragments[0]),
        [true, true, true, true]
    );

    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(draft_piece_fragment_is_stored_exactly(
        &store,
        &storage,
        &edit.fragments[0],
    ));
}

#[test]
fn large_continued_edit_advances_only_the_named_candidate() {
    let (home, store, storage, thread) = fixture("candidate-only", 10);
    let durable_before = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable_before, 20, 21);
    let first = "a".repeat(60_000);
    let second = "é".repeat(20_000);
    let fragments = vec![
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text(first.clone())]),
        DraftPieceReplacementV1::continuation(
            point(0),
            point(0),
            vec![DraftPieceV1::Text(second.clone())],
        ),
    ];
    let edit = transaction(&storage, &store, &session, 22, fragments, point(100_000));
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    for fragment in &edit.fragments {
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    let first_advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
        .expect("large edit unexpectedly completed before its first advance");
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), first_advance),
    ));
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(&storage, &store, &edit);
    let DraftPieceSettlementOutcomeV1::Committed {
        candidate_generation,
        successor,
        ..
    } = settlement.outcome()
    else {
        panic!("candidate edit did not commit")
    };
    assert_eq!(*candidate_generation, 1);
    assert_eq!(successor.summary().logical_utf8_bytes(), 100_000);

    let adopted = match settlement.closure() {
        syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
            remember_fixture_positions(
                adoption.adopted_session(),
                FixturePositions {
                    caret: adoption.transition().after_caret(),
                    selection: adoption.transition().after_selection(),
                },
            );
            adoption.adopted_session().clone()
        }
        _ => panic!("committed settlement lacked adoption closure"),
    };
    let expected = format!("{first}{second}").into_bytes();
    assert_eq!(candidate_bytes(&storage, &store, &adopted), expected);
    assert_eq!(current(&storage, &store, thread), durable_before);

    let deletion = transaction(
        &storage,
        &store,
        &adopted,
        23,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(100_000),
            Vec::new(),
        )],
        point(0),
    );
    begin_stage_build(&storage, &store, &deletion);
    committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), deletion.prepared.clone()),
    ));
    let emptied = adopted_head(&storage, &store, &deletion);
    assert_eq!(emptied.newest_candidate_generation(), 2);
    assert!(candidate_bytes(&storage, &store, &emptied).is_empty());
    assert_eq!(current(&storage, &store, thread), durable_before);

    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        current(&reopened_storage, &reopened, thread),
        durable_before
    );
    let fresh = open_session(&reopened_storage, &reopened, &durable_before, 24, 25);
    assert_eq!(fresh.newest_candidate_generation(), 0);
    assert!(candidate_bytes(&reopened_storage, &reopened, &fresh).is_empty());
}

#[test]
fn replay_collision_cancellation_and_old_session_isolation_are_closed() {
    let (_home, store, storage, thread) = fixture("terminals", 50);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 51, 52);
    let cancelled = transaction(
        &storage,
        &store,
        &session,
        53,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("cancel".to_owned())],
        )],
        point(6),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    let claimed = active_session(&storage, &store, &session);
    assert_eq!(
        claimed
            .active_operation()
            .unwrap()
            .build_receipt()
            .unwrap()
            .key()
            .transition_ordinal(),
        1
    );
    replay_succeeded(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
            cancelled.fragments[0].clone(),
        ),
    ));
    let staged = active_session(&storage, &store, &session);
    assert_eq!(
        staged
            .active_operation()
            .unwrap()
            .build_receipt()
            .unwrap()
            .key()
            .transition_ordinal(),
        2
    );
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    assert_eq!(active_session(&storage, &store, &session), staged);
    replay_succeeded(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
            cancelled.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    assert!(matches!(
        settled(&storage, &store, &cancelled).outcome(),
        DraftPieceSettlementOutcomeV1::Cancelled
    ));
    assert!(
        active_session(&storage, &store, &session)
            .active_operation()
            .is_none()
    );

    let session_after_cancel = active_session(&storage, &store, &session);
    let accepted = transaction(
        &storage,
        &store,
        &session_after_cancel,
        54,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("accepted".to_owned())],
        )],
        point(8),
    );
    begin_stage_build(&storage, &store, &accepted);
    committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), accepted.prepared.clone()),
    ));
    assert!(matches!(
        settled(&storage, &store, &accepted).outcome(),
        DraftPieceSettlementOutcomeV1::Committed { .. }
    ));
    let accepted_session = adopted_head(&storage, &store, &accepted);
    let colliding = transaction(
        &storage,
        &store,
        &accepted_session,
        54,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("different".to_owned())],
        )],
        point(9),
    );
    assert!(matches!(
        status(&storage, &store, &colliding),
        DraftPieceOperationStatusV1::Collision(_)
    ));

    let isolated = open_session(&storage, &store, &durable, 55, 56);
    let isolated_edit = transaction(
        &storage,
        &store,
        &isolated,
        57,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("isolated".to_owned())],
        )],
        point(8),
    );
    begin_stage_build(&storage, &store, &isolated_edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            isolated_edit.prepared.clone(),
        ),
    ));
    assert_eq!(
        candidate_bytes(&storage, &store, &adopted_head(&storage, &store, &accepted)),
        b"accepted"
    );
    assert_eq!(
        candidate_bytes(
            &storage,
            &store,
            &adopted_head(&storage, &store, &isolated_edit)
        ),
        b"isolated"
    );
    assert_eq!(current(&storage, &store, thread), durable);
}

#[test]
fn partial_and_terminal_first_cancellation_reconcile_staged_endpoints() {
    let (_home, store, storage, thread) = fixture("partial-terminal-cancellation", 58);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 59, 60);
    let partial = transaction(
        &storage,
        &store,
        &session,
        61,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("one".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("two".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("three".to_owned())],
            ),
        ],
        point(13),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            partial.prepared.clone(),
            partial.fragments[0].clone(),
        ),
    ));
    let partial_fragments = partial.fragments.clone();
    let partial_outcome = execute(
        &store,
        storage
            .cancel_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(
                &store,
                &partial.prepared,
                partial_outcome,
                |start| partial_fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect(),
            )
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        )
    ));
    let partial_settlement = settled(&storage, &store, &partial);
    assert_eq!(partial_settlement.fragment_count(), 3);
    let partial_source = partial_settlement
        .terminal_source()
        .expect("partial cancellation retains its exact staged source");
    assert_eq!(partial_source.staged_fragment_count(), 1);
    assert_eq!(
        partial_source.staged_fragment_chain(),
        partial.fragments[0].chain_digest()
    );
    assert_eq!(
        partial_settlement
            .terminal_receipt()
            .key()
            .transition_ordinal(),
        3
    );
    assert!(
        active_session(&storage, &store, &session)
            .active_operation()
            .is_none()
    );
    replay_succeeded(execute(
        &store,
        storage
            .cancel_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    ));
    not_committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    ));
    not_committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            partial.prepared.clone(),
            DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    assert_eq!(settled(&storage, &store, &partial), partial_settlement);

    let terminal_first_source = active_session(&storage, &store, &session);
    let terminal_first = transaction(
        &storage,
        &store,
        &terminal_first_source,
        62,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("never staged".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("also unstaged".to_owned())],
            ),
        ],
        point(25),
    );
    let terminal_first_fragments = terminal_first.fragments.clone();
    let terminal_first_outcome = execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(
                &store,
                &terminal_first.prepared,
                terminal_first_outcome,
                |start| terminal_first_fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect(),
            )
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        )
    ));
    let terminal_first_settlement = settled(&storage, &store, &terminal_first);
    assert_eq!(terminal_first_settlement.fragment_count(), 2);
    assert!(terminal_first_settlement.terminal_source().is_none());
    assert_eq!(
        terminal_first_settlement
            .terminal_receipt()
            .key()
            .transition_ordinal(),
        1
    );
    assert!(
        active_session(&storage, &store, &session)
            .active_operation()
            .is_none()
    );
    replay_succeeded(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
            DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
        ),
    ));
    assert_eq!(
        settled(&storage, &store, &terminal_first),
        terminal_first_settlement
    );
    assert_eq!(current(&storage, &store, thread), durable);
}

#[test]
fn terminal_first_rejects_a_clean_session_generation_race_before_claim() {
    let (_home, store, storage, thread) = fixture("terminal-first-source-race", 63);
    let durable = current(&storage, &store, thread);
    let source = open_session(&storage, &store, &durable, 64, 65);
    let stale = transaction(
        &storage,
        &store,
        &source,
        66,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("stale".to_owned())],
        )],
        point(5),
    );
    let intervening = transaction(
        &storage,
        &store,
        &source,
        67,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("intervening".to_owned())],
        )],
        point(12),
    );
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            intervening.prepared.clone(),
        ),
    ));
    assert!(matches!(
        status(&storage, &store, &intervening),
        DraftPieceOperationStatusV1::Settled(_)
    ));
    let advanced = active_session(&storage, &store, &source);
    assert_eq!(
        advanced.session_generation(),
        source.session_generation() + 2
    );
    assert_eq!(
        advanced.newest_candidate_generation(),
        source.newest_candidate_generation()
    );
    assert_eq!(advanced.newest_root(), source.newest_root());
    assert_eq!(current(&storage, &store, thread), durable);

    let revision = storage.revision(&store).unwrap();
    not_committed(execute(
        &store,
        storage.cancel_draft_piece_edit(revision, stale.prepared.clone()),
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(active_session(&storage, &store, &source), advanced);
    assert!(matches!(
        status(&storage, &store, &stale),
        DraftPieceOperationStatusV1::Absent
    ));

    let fresh = transaction(
        &storage,
        &store,
        &advanced,
        66,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("stale".to_owned())],
        )],
        point(5),
    );
    let fresh_fragments = fresh.fragments.clone();
    let outcome = execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), fresh.prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &fresh.prepared, outcome, |start| {
                fresh_fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect()
            },)
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        )
    ));
    assert!(
        settled(&storage, &store, &fresh)
            .terminal_source()
            .is_none()
    );
    assert_eq!(current(&storage, &store, thread), durable);
}

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_begin_fragment_advance_and_adoption_reconcile_from_durable_identity() {
    let home = TestHome::new("fault-cuts");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&storage, &store, 70);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 71, 72);
    let edit = transaction(
        &storage,
        &store,
        &session,
        73,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("faulted".to_owned())],
        )],
        point(7),
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let begin = execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    assert!(matches!(begin, CommandOutcome::Indeterminate { .. }));
    assert!(matches!(
        status(&storage, &store, &edit),
        DraftPieceOperationStatusV1::Open(_)
    ));

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let staged = execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    );
    assert!(matches!(staged, CommandOutcome::Indeterminate { .. }));
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
    {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let outcome = execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        );
        assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
    }
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    let fragments = edit.fragments.clone();
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &edit.prepared, outcome, |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Committed(_)
        )
    ));
    assert_eq!(current(&storage, &store, thread), durable);

    let disposable = open_session(&storage, &store, &durable, 74, 75);
    let stale_completion = transaction(
        &storage,
        &store,
        &disposable,
        76,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("disposed".to_owned())],
        )],
        point(8),
    );
    begin_stage_build(&storage, &store, &stale_completion);
    not_committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            disposable.draft_id(),
            disposable.session_id(),
        ),
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            stale_completion.prepared.clone(),
        ),
    ));
    assert!(matches!(
        settled(&storage, &store, &stale_completion).outcome(),
        DraftPieceSettlementOutcomeV1::Cancelled
    ));
    committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            disposable.draft_id(),
            disposable.session_id(),
        ),
    ));

    let after_disposal = transaction(
        &storage,
        &store,
        &disposable,
        77,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("absent".to_owned())],
        )],
        point(6),
    );
    assert!(matches!(
        execute(
            &store,
            storage.begin_draft_piece_edit(
                storage.revision(&store).unwrap(),
                after_disposal.prepared.clone(),
            ),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        status(&storage, &store, &after_disposal),
        DraftPieceOperationStatusV1::Absent
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn occupied_roots_and_impossible_session_or_build_records_fail_closed() {
    for (case, collision) in [
        ("exact-root", DraftPieceCandidateRootCollision::Exact),
        (
            "different-root",
            DraftPieceCandidateRootCollision::DifferentCanonicalBytes,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 80);
        let durable = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &durable, 81, 82);
        let edit = transaction(
            &storage,
            &store,
            &session,
            83,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("root".to_owned())],
            )],
            point(4),
        );
        begin_stage_build(&storage, &store, &edit);
        let DraftPieceOperationStatusV1::Complete(build) = status(&storage, &store, &edit) else {
            panic!("fixture build is not complete")
        };
        let successor = build.successor().unwrap();
        let collision_outcome = execute(
            &store,
            inject_draft_piece_candidate_root_collision(&store, &storage, successor, collision),
        );
        committed(collision_outcome);
        let session_before = active_session(&storage, &store, &session);
        not_committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        assert_eq!(active_session(&storage, &store, &session), session_before);
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments)
                .is_err()
        );
        not_committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        assert_eq!(current(&storage, &store, thread), durable);
    }

    let (_home, store, storage, thread) = fixture("session-frontier-corruption", 84);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 85, 86);
    committed(execute(
        &store,
        inject_draft_editor_candidate_session_published_beyond_newest(
            &store,
            &storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    assert!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .is_err()
    );

    for (case, corruption, settle_first) in [
        (
            "malformed-open-build",
            DraftPieceBuildCorruption::OpenWithCompleteFrontier,
            false,
        ),
        (
            "malformed-complete-build",
            DraftPieceBuildCorruption::CompleteWithoutSuccessor,
            false,
        ),
        (
            "malformed-terminal-build",
            DraftPieceBuildCorruption::CompleteWithoutSuccessor,
            true,
        ),
        (
            "disagreeing-terminal-build",
            DraftPieceBuildCorruption::TerminalLifecycle,
            true,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 87);
        let durable = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &durable, 88, 89);
        let edit = transaction(
            &storage,
            &store,
            &session,
            90,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("build".to_owned())],
            )],
            point(5),
        );
        begin_stage_build(&storage, &store, &edit);
        if settle_first {
            committed(execute(
                &store,
                storage.settle_draft_piece_edit(
                    storage.revision(&store).unwrap(),
                    edit.prepared.clone(),
                ),
            ));
        }
        committed(execute(
            &store,
            inject_draft_piece_build_corruption(
                &store,
                &storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    edit.prepared.header().draft_id(),
                    edit.prepared.header().session_id(),
                    edit.prepared.header().operation_id(),
                ),
                corruption,
            ),
        ));
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments,)
                .is_err(),
            "corruption {corruption:?} was accepted"
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn candidate_adoption_closure_and_open_receipt_bytes_fail_closed() {
    for case in [
        "missing-root",
        "missing-settlement",
        "missing-build",
        "bad-settlement",
    ] {
        let (_home, store, storage, thread) = fixture(case, 120);
        let durable = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &durable, 121, 122);
        let first = transaction(
            &storage,
            &store,
            &session,
            123,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("first".to_owned())],
            )],
            point(5),
        );
        begin_stage_build(&storage, &store, &first);
        committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), first.prepared.clone()),
        ));
        let first_head = adopted_head(&storage, &store, &first);
        let second = transaction(
            &storage,
            &store,
            &first_head,
            124,
            vec![DraftPieceReplacementV1::new(
                point(5),
                point(5),
                vec![DraftPieceV1::Text("-second".to_owned())],
            )],
            point(12),
        );
        begin_stage_build(&storage, &store, &second);
        committed(execute(
            &store,
            storage.settle_draft_piece_edit(
                storage.revision(&store).unwrap(),
                second.prepared.clone(),
            ),
        ));
        let head = adopted_head(&storage, &store, &second);
        let pending = transaction(
            &storage,
            &store,
            &head,
            125,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("pending".to_owned())],
            )],
            point(7),
        );
        let pending_contribution = storage
            .begin_draft_piece_edit(storage.revision(&store).unwrap(), pending.prepared.clone());
        let key = syndic_storage::DraftPieceSettlementKeyV1::new(
            head.draft_id(),
            head.session_id(),
            head.newest_root().key().operation_id(),
        );
        let corruption = match case {
            "missing-root" => delete_draft_piece_immutable_record(
                &store,
                &storage,
                head.newest_root(),
                DraftPieceImmutableDeletion::Root,
            ),
            "missing-settlement" => delete_draft_piece_immutable_record(
                &store,
                &storage,
                head.newest_root(),
                DraftPieceImmutableDeletion::Settlement,
            ),
            "missing-build" => delete_draft_piece_terminal_build(&store, &storage, key),
            "bad-settlement" => {
                inject_draft_piece_settlement_closure_corruption(&store, &storage, key)
            }
            _ => unreachable!(),
        };
        committed(execute(&store, corruption));
        assert!(
            !matches!(
                storage.draft_editor_candidate_session(&store, head.draft_id(), head.session_id(),),
                Ok(DraftEditorCandidateSessionReadOutcomeV1::Active(_))
            ),
            "candidate session accepted {case}"
        );
        assert!(
            storage
                .candidate_draft_piece_text_demand(
                    &store,
                    DraftEditorCandidateActivationBindingV1::from_head(&head),
                    DraftPieceTextDemandV1::Forward(0),
                    65_536,
                )
                .is_err(),
            "candidate range accepted {case}"
        );
        if store.home_revision().is_ok() {
            assert!(matches!(
                execute(&store, pending_contribution),
                CommandOutcome::NotCommitted { .. }
            ));
        } else {
            assert!(storage.revision(&store).is_err());
        }
    }

    for (case, corruption) in [
        (
            "receipt-malformed",
            DraftEditorCandidateOpenReceiptCorruption::Malformed,
        ),
        (
            "receipt-truncated",
            DraftEditorCandidateOpenReceiptCorruption::Truncated,
        ),
        (
            "receipt-noncanonical",
            DraftEditorCandidateOpenReceiptCorruption::Noncanonical,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 126);
        let durable = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &durable, 127, 128);
        let request = DraftEditorCandidateSessionOpenRequestV1::new(
            selector(&durable),
            session.session_id(),
            session.open_operation_id(),
        );
        let prepared = storage
            .prepare_open_draft_editor_candidate_session(&store, request)
            .unwrap();
        let outcome = execute(
            &store,
            storage.open_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        committed(execute(
            &store,
            inject_draft_editor_candidate_open_receipt_corruption(
                &store,
                &storage,
                session.draft_id(),
                session.session_id(),
                session.open_operation_id(),
                corruption,
            ),
        ));
        assert!(
            storage
                .draft_editor_candidate_session(&store, session.draft_id(), session.session_id(),)
                .is_err()
        );
        assert!(
            storage
                .prepare_open_draft_editor_candidate_session(&store, request)
                .is_err()
        );
        assert!(
            storage
                .reconcile_draft_editor_candidate_session_open(&store, &prepared, outcome)
                .is_err()
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn realistic_in_range_build_phase_jumps_fail_progress_authentication() {
    for (case, corruption, target) in [
        (
            "jump-cross-validating",
            DraftPieceBuildCorruption::StagedToCrossValidating,
            1_u8,
        ),
        (
            "jump-next-piece",
            DraftPieceBuildCorruption::InsertingNextPieceInRange,
            4,
        ),
        (
            "jump-next-byte",
            DraftPieceBuildCorruption::InsertingScalarByteSkip,
            4,
        ),
        (
            "jump-planning-applying",
            DraftPieceBuildCorruption::PlanningToApplying,
            1,
        ),
        (
            "jump-removing-applying",
            DraftPieceBuildCorruption::RemovingToApplying,
            2,
        ),
        (
            "jump-applying-inserting",
            DraftPieceBuildCorruption::ApplyingToInserting,
            3,
        ),
        (
            "jump-adjacent-fragment",
            DraftPieceBuildCorruption::AdjacentFragmentJump,
            1,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 130);
        let durable = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &durable, 131, 132);
        let seed = transaction(
            &storage,
            &store,
            &session,
            133,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("ab".to_owned())],
            )],
            point(2),
        );
        begin_stage_build(&storage, &store, &seed);
        committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
        ));
        let seeded = adopted_head(&storage, &store, &seed);
        let edit = transaction(
            &storage,
            &store,
            &seeded,
            134,
            vec![
                DraftPieceReplacementV1::new(
                    point(0),
                    point(1),
                    vec![
                        DraftPieceV1::Text("éx".to_owned()),
                        DraftPieceV1::Text("tail".to_owned()),
                    ],
                ),
                DraftPieceReplacementV1::new(
                    point(1),
                    point(2),
                    vec![DraftPieceV1::Text("z".to_owned())],
                ),
            ],
            point(1),
        );
        committed(execute(
            &store,
            storage
                .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        for fragment in &edit.fragments {
            committed(execute(
                &store,
                storage.stage_draft_piece_fragment(
                    storage.revision(&store).unwrap(),
                    edit.prepared.clone(),
                    fragment.clone(),
                ),
            ));
        }
        loop {
            let DraftPieceOperationStatusV1::Open(build) = status(&storage, &store, &edit) else {
                panic!("phase-jump fixture stopped being open")
            };
            let reached = matches!(
                (target, build.frontier()),
                (
                    1,
                    DraftPieceBuildFrontierV1::Planning {
                        fragment_ordinal: 1
                    }
                ) | (
                    2,
                    DraftPieceBuildFrontierV1::Removing {
                        fragment_ordinal: 1,
                        ..
                    }
                ) | (
                    3,
                    DraftPieceBuildFrontierV1::Applying {
                        fragment_ordinal: 1,
                        ..
                    }
                ) | (
                    4,
                    DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal: 1,
                        ..
                    }
                )
            );
            if reached {
                break;
            }
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    seeded.draft_id(),
                    seeded.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .unwrap_or_else(|error| panic!("{case} failed before corruption: {error:?}"))
                .unwrap();
            committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            ));
        }
        committed(execute(
            &store,
            inject_draft_piece_build_corruption(
                &store,
                &storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    seeded.draft_id(),
                    seeded.session_id(),
                    edit.prepared.header().operation_id(),
                ),
                corruption,
            ),
        ));
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    seeded.draft_id(),
                    seeded.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .is_err(),
            "phase corruption {corruption:?} advanced"
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn immutable_progress_receipt_endpoint_closes_advance_status_and_settlement() {
    for (case, corruption) in [
        (
            "progress-delete",
            DraftPieceProgressReceiptCorruption::Delete,
        ),
        (
            "progress-delete-previous",
            DraftPieceProgressReceiptCorruption::DeletePrevious,
        ),
        (
            "progress-state-mismatch",
            DraftPieceProgressReceiptCorruption::StateMismatch,
        ),
        (
            "progress-previous-state-mismatch",
            DraftPieceProgressReceiptCorruption::PreviousStateMismatch,
        ),
        (
            "progress-head-mismatch",
            DraftPieceProgressReceiptCorruption::HeadEndpointMismatch,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 138);
        let durable = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &durable, 139, 140);
        let edit = transaction(
            &storage,
            &store,
            &session,
            141,
            vec![
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("receipt".to_owned())],
                ),
                DraftPieceReplacementV1::continuation(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("tail".to_owned())],
                ),
            ],
            point(7),
        );
        committed(execute(
            &store,
            storage
                .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                edit.fragments[0].clone(),
            ),
        ));
        let key = syndic_storage::DraftPieceSettlementKeyV1::new(
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        );
        committed(execute(
            &store,
            inject_draft_piece_progress_receipt_corruption(&store, &storage, key, corruption),
        ));
        not_committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                edit.fragments[1].clone(),
            ),
        ));
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .is_err(),
            "receipt corruption {corruption:?} advanced"
        );
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments[..1],)
                .is_err(),
            "receipt corruption {corruption:?} produced status"
        );
        assert!(matches!(
            storage
                .draft_editor_candidate_session(&store, session.draft_id(), session.session_id(),)
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
        ));
        assert!(
            !matches!(
                execute(
                    &store,
                    storage.settle_draft_piece_edit(
                        storage.revision(&store).unwrap(),
                        edit.prepared.clone(),
                    ),
                ),
                CommandOutcome::Committed { .. }
            ),
            "receipt corruption {corruption:?} settled"
        );
    }

    let (_home, store, storage, thread) = fixture("progress-forged-cross-validating", 152);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 153, 154);
    let edit = transaction(
        &storage,
        &store,
        &session,
        155,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("forged".to_owned())],
        )],
        point(6),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        inject_draft_piece_build_corruption(
            &store,
            &storage,
            syndic_storage::DraftPieceSettlementKeyV1::new(
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            ),
            DraftPieceBuildCorruption::StagedToCrossValidating,
        ),
    ));
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    if let Ok(revision) = storage.revision(&store) {
        assert!(!matches!(
            execute(
                &store,
                storage.settle_draft_piece_edit(revision, edit.prepared.clone()),
            ),
            CommandOutcome::Committed { .. }
        ));
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn progress_target_fork_fragment_ahead_and_custody_drift_fail_closed() {
    let (_home, store, storage, thread) = fixture("occupied-progress-target", 156);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 157, 158);
    let edit = transaction(
        &storage,
        &store,
        &session,
        159,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("fork".to_owned())],
        )],
        point(4),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let key = syndic_storage::DraftPieceSettlementKeyV1::new(
        session.draft_id(),
        session.session_id(),
        edit.prepared.header().operation_id(),
    );
    committed(execute(
        &store,
        inject_draft_piece_occupied_stage_target(&store, &storage, key, edit.fragments[0].clone()),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));

    let (_home, store, storage, thread) = fixture("fragment-ahead", 160);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 161, 162);
    let edit = transaction(
        &storage,
        &store,
        &session,
        163,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("ahead".to_owned())],
        )],
        point(5),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        inject_draft_piece_fragment_ahead(&store, &storage, edit.fragments[0].clone()),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));

    let (_home, store, storage, thread) = fixture("custody-endpoint-drift", 164);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 165, 166);
    let edit = transaction(
        &storage,
        &store,
        &session,
        167,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("custody".to_owned())],
        )],
        point(7),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        inject_draft_piece_custody_endpoint_corruption(
            &store,
            &storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn begin_replay_rejects_codec_valid_session_generation_inflation() {
    let (_home, store, storage, thread) = fixture("begin-session-generation", 180);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 181, 182);
    let edit = transaction(
        &storage,
        &store,
        &session,
        183,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("begin".to_owned())],
        )],
        point(5),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        inject_draft_piece_session_generation_inflation(
            &store,
            &storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn stage_replay_rejects_codec_valid_session_generation_inflation() {
    let (_home, store, storage, thread) = fixture("stage-session-generation", 184);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 185, 186);
    let edit = transaction(
        &storage,
        &store,
        &session,
        187,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("stage".to_owned())],
        )],
        point(5),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        inject_draft_piece_session_generation_inflation(
            &store,
            &storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn advance_replay_rejects_codec_valid_session_generation_inflation() {
    let (_home, store, storage, thread) = fixture("advance-session-generation", 188);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 189, 190);
    let edit = transaction(
        &storage,
        &store,
        &session,
        191,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("advance".to_owned())],
        )],
        point(7),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    let advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
        .expect("fixture has one build advance");
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance.clone()),
    ));
    committed(execute(
        &store,
        inject_draft_piece_session_generation_inflation(
            &store,
            &storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn stage_replay_rejects_coordinated_codec_valid_target_replacement() {
    let (_home, store, storage, thread) = fixture("stage-coordinated-target", 192);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 193, 194);
    let edit = transaction(
        &storage,
        &store,
        &session,
        195,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("closure".to_owned())],
        )],
        point(7),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    let key = syndic_storage::DraftPieceSettlementKeyV1::new(
        session.draft_id(),
        session.session_id(),
        edit.prepared.header().operation_id(),
    );
    committed(execute(
        &store,
        inject_draft_piece_coordinated_stage_target_replacement(&store, &storage, key),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
}

fn transaction(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let predecessor_positions = fixture_positions(session);
    let operation = DraftPieceOperationIdV1::from_bytes([operation; 16]);
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        operation,
        predecessor_positions.caret,
        predecessor_positions.selection,
        caret,
        caret,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, session)
        .unwrap();
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let fragments = replacements
        .into_iter()
        .enumerate()
        .map(|(ordinal, replacement)| {
            let fragment = storage
                .prepare_draft_piece_fragment(&prepared, ordinal as u64 + 1, preceding, replacement)
                .unwrap();
            preceding = fragment.chain_digest();
            fragment
        })
        .collect();
    Transaction {
        prepared,
        fragments,
    }
}

fn begin_stage_build(storage: &SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
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
}

fn build_and_reject(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftPieceRejectedReasonV1 {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
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
    let reason = loop {
        match storage.prepare_draft_piece_build_advance(
            store,
            transaction.prepared.header().draft_id(),
            transaction.prepared.header().session_id(),
            transaction.prepared.header().operation_id(),
        ) {
            Ok(Some(advance)) => committed(execute(
                store,
                storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
            )),
            Err(DraftPiecePrepareErrorV1::Rejected(reason)) => break reason,
            Ok(None) => panic!("invalid marker edit unexpectedly completed"),
            Err(error) => panic!("unexpected marker edit build error: {error:?}"),
        }
    };
    committed(execute(
        store,
        storage.reject_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
            reason,
        ),
    ));
    assert!(matches!(
        settled(&storage, store, transaction).outcome(),
        DraftPieceSettlementOutcomeV1::Rejected(actual) if *actual == reason
    ));
    reason
}

fn settled(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> syndic_storage::DraftPieceSettlementV1 {
    match status(&storage, store, transaction) {
        DraftPieceOperationStatusV1::Settled(settlement) => settlement,
        other => panic!("operation is not settled: {other:?}"),
    }
}

fn status(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftPieceOperationStatusV1 {
    match storage
        .draft_piece_operation_status_page(store, &transaction.prepared, 1, &transaction.fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(status) => status,
        DraftPieceOperationVerificationV1::More { .. } => panic!("small proposal did not verify"),
    }
}

fn adopted_head(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftEditorCandidateSessionV1 {
    match settled(&storage, store, transaction).closure() {
        syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
            remember_fixture_positions(
                adoption.adopted_session(),
                FixturePositions {
                    caret: adoption.transition().after_caret(),
                    selection: adoption.transition().after_selection(),
                },
            );
            adoption.adopted_session().clone()
        }
        _ => panic!("operation was not adopted"),
    }
}

fn active_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    expected: &DraftEditorCandidateSessionV1,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(store, expected.draft_id(), expected.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        other => panic!("candidate session is not active: {other:?}"),
    }
}

fn settled_head(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftEditorCandidateSessionV1 {
    match settled(&storage, store, transaction).closure() {
        syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
            remember_fixture_positions(
                adoption.adopted_session(),
                FixturePositions {
                    caret: adoption.transition().after_caret(),
                    selection: adoption.transition().after_selection(),
                },
            );
            adoption.adopted_session().clone()
        }
        syndic_storage::DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
            noncommit.observed_session().clone()
        }
    }
}

fn candidate_bytes(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Vec<u8> {
    let binding = DraftEditorCandidateActivationBindingV1::from_head(head);
    let mut offset = 0;
    let mut bytes = Vec::new();
    loop {
        let page = storage
            .candidate_draft_piece_text_demand(
                store,
                binding,
                DraftPieceTextDemandV1::Forward(offset),
                65_536,
            )
            .unwrap()
            .value()
            .clone();
        bytes.extend_from_slice(page.bytes());
        match page.following() {
            syndic_storage::DraftPieceTextEdgeFactV1::Continuation(next) => offset = next,
            syndic_storage::DraftPieceTextEdgeFactV1::DocumentEnd => break,
            _ => panic!("forward page returned a preceding edge"),
        }
    }
    bytes
}

fn open_session(
    storage: &SyndicStorage,
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

fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&storage, &store, seed);
    (home, store, storage, thread)
}

fn create_thread(storage: &SyndicStorage, store: &HomeStore, seed: u8) -> SyndicThreadId {
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase142",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    thread
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

fn not_committed(outcome: CommandOutcome) {
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
}

fn replay_succeeded(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted { .. }
            | CommandOutcome::Committed {
                later_failure: None,
                ..
            }
    ));
}

fn current(
    storage: &SyndicStorage,
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

fn marker(seed: u8, order: u64, label: u64) -> DraftPieceMarkerV1 {
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([seed; 16]),
        order,
        ImageLabelOrdinal::new(label).unwrap(),
        beryl_model::AssetId::sha256_v1([seed; 32], std::num::NonZeroU64::new(label).unwrap()),
    )
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}
