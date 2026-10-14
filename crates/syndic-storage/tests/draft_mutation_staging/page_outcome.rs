use super::*;
use syndic_storage::DraftMutationStagingPageOutcomeErrorV1;

fn pending_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> PreparedDraftMutationStagingBatchV1 {
    let current = current(storage, store, thread);
    let session = open_session(storage, store, &current, 241, 242);
    let identity = staging_identity(&session, 243);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    let active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin.clone()),
    ));
    prepare_one_page_batch(
        storage,
        begin.target_head(),
        &active,
        DraftMutationStagingLaneV1::Proposal,
        1,
        1,
        65_536,
        Box::new([DraftMutationStagingPageItemV1::Proposal(
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("retained payload".repeat(1024))],
            ),
        )]),
    )
    .unwrap()
}

fn fault_fixture(
    name: &str,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    SyndicThreadId,
    FaultController,
) {
    let home = TestHome::new(name);
    let faults = FaultController::new();
    let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([239; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([240; 16]),
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread, faults)
}

#[test]
fn committed_page_keeps_one_payload_and_receipt_across_exact_read_retry() {
    let (_home, store, storage, thread) = fixture("page-flight-committed", 201);
    let prepared = pending_page(&storage, &store, thread);
    let witness = prepared.clone();
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
        )
        .unwrap();
    assert!(flight.shares_payload_for_test(&witness));
    let receipt = flight.committed_receipt().unwrap().clone();
    let revision = store.home_revision().unwrap();
    for _ in 0..2 {
        flight.fail_next_exact_read_for_test();
        assert!(matches!(
            flight.resolve(&store),
            Err(DraftMutationStagingPageOutcomeErrorV1::Staging(
                DraftMutationStagingErrorV1::Read(_)
            ))
        ));
        assert_eq!(flight.committed_receipt(), Some(&receipt));
        assert!(flight.shares_payload_for_test(&witness));
        flight = match flight.into_prepared() {
            Err(retained) => retained,
            Ok(_) => panic!("unresolved target released retry payload"),
        };
    }
    assert_eq!(
        flight.resolve(&store).unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected
    );
    assert_eq!(
        flight.resolve(&store).unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected
    );
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(flight.into_prepared().is_err());
}

#[test]
fn definitive_source_keeps_original_failure_then_releases_exact_retry_payload() {
    let (_home, store, storage, thread) = fixture("page-flight-source", 202);
    let stale = storage.revision(&store).unwrap();
    let prepared = pending_page(&storage, &store, thread);
    let target = prepared.target_head().clone();
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch(&store, stale, prepared)
        .unwrap();
    assert!(flight.is_not_committed());
    let original = flight.original_failure().unwrap() as *const _;
    flight.fail_next_exact_read_for_test();
    assert!(flight.resolve(&store).is_err());
    assert_eq!(flight.original_failure().unwrap() as *const _, original);
    assert_eq!(
        flight.resolve(&store).unwrap(),
        DraftMutationStagingReconcileV1::SourceSelected
    );
    let prepared = match flight.into_prepared() {
        Ok(prepared) => prepared,
        Err(_) => panic!("exact source did not release payload"),
    };
    assert_eq!(prepared.target_head(), &target);
    let mut retry = storage
        .submit_draft_mutation_staging_page_batch(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
        )
        .unwrap();
    assert_eq!(
        retry.resolve(&store).unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected
    );
}

#[test]
fn indeterminate_page_retries_original_handle_and_keeps_target_expectation_after_read_error() {
    let (_home, store, storage, thread, faults) = fault_fixture("page-flight-reconciliation");
    let prepared = pending_page(&storage, &store, thread);
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch_with_before_execute_faults(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
            || faults.fail_next(FaultPoint::AfterCommitBeforePersist),
        )
        .unwrap();
    assert!(flight.reconciliation_handle_present());
    let original = flight.original_failure().unwrap() as *const _;
    for _ in 0..2 {
        faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
        assert!(matches!(
            flight.resolve(&store),
            Err(DraftMutationStagingPageOutcomeErrorV1::Staging(
                DraftMutationStagingErrorV1::Reconciliation(_)
            ))
        ));
        assert!(flight.reconciliation_handle_present());
        assert_eq!(flight.original_failure().unwrap() as *const _, original);
        flight = match flight.into_prepared() {
            Err(retained) => retained,
            Ok(_) => panic!("unresolved reconciliation released payload"),
        };
    }
    flight.fail_next_exact_read_for_test();
    assert!(flight.resolve(&store).is_err());
    assert!(flight.committed_receipt().is_some());
    assert!(flight.reconciliation_handle_present());
    let receipt = flight.committed_receipt().unwrap().clone();
    assert_eq!(
        flight.resolve(&store).unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected
    );
    assert_eq!(flight.committed_receipt(), Some(&receipt));
    assert_eq!(flight.original_failure().unwrap() as *const _, original);
}

#[test]
fn foreign_home_preserves_flight_and_rejected_submission_returns_unexecuted_page() {
    let (_home, store, storage, thread) = fixture("page-flight-owner", 203);
    let (_foreign_home, foreign, _, _) = fixture("page-flight-foreign", 204);
    let prepared = pending_page(&storage, &store, thread);
    let source_revision = store.home_revision().unwrap();
    let failure = match storage.submit_draft_mutation_staging_page_batch(
        &foreign,
        storage.revision(&store).unwrap(),
        prepared,
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("foreign submission accepted Syndic handle"),
    };
    let (prepared, _) = failure.into_parts();
    assert_eq!(store.home_revision().unwrap(), source_revision);
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
        )
        .unwrap();
    let receipt = flight.committed_receipt().unwrap().clone();
    assert!(matches!(
        flight.resolve(&foreign),
        Err(DraftMutationStagingPageOutcomeErrorV1::ForeignHome)
    ));
    assert_eq!(flight.committed_receipt(), Some(&receipt));
    assert_eq!(
        flight.resolve(&store).unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected
    );
}

#[test]
fn failed_generation_retains_committed_receipt_and_later_failure() {
    let (_home, store, storage, thread, faults) = fault_fixture("page-flight-health");
    let prepared = pending_page(&storage, &store, thread);
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
        )
        .unwrap();
    let receipt = flight.committed_receipt().unwrap().clone();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    for _ in 0..2 {
        assert!(flight.resolve(&store).is_err());
        assert_eq!(flight.committed_receipt(), Some(&receipt));
        assert!(!flight.local_finalization_consumed());
        flight = match flight.into_prepared() {
            Err(retained) => retained,
            Ok(_) => panic!("failed generation released retry payload"),
        };
    }
}

#[test]
fn committed_structural_failure_retains_original_local_finalization_capability() {
    let (_home, store, storage, thread, faults) = fault_fixture("page-flight-finalization");
    let prepared = pending_page(&storage, &store, thread);
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch_with_before_execute_faults(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
            || faults.fail_next(FaultPoint::AfterPersist),
        )
        .unwrap();
    assert!(flight.committed_receipt().is_some());
    assert!(flight.later_failure().is_some());
    assert!(flight.has_pending_local_finalization_for_test());
    let original = flight.later_failure().unwrap() as *const _;
    for _ in 0..2 {
        assert!(flight.resolve(&store).is_err());
        assert!(flight.has_pending_local_finalization_for_test());
        assert!(!flight.local_finalization_consumed());
        assert_eq!(flight.later_failure().unwrap() as *const _, original);
    }
}

#[test]
fn exact_target_page_corruption_never_becomes_source_or_retry_authority() {
    let (_home, store, storage, thread) = fixture("page-flight-corruption", 205);
    let prepared = pending_page(&storage, &store, thread);
    let (target_page, _) = draft_mutation_staging_batch_target(&prepared, 0).unwrap();
    let mut flight = storage
        .submit_draft_mutation_staging_page_batch(
            &store,
            storage.revision(&store).unwrap(),
            prepared,
        )
        .unwrap();
    let receipt = flight.committed_receipt().unwrap().clone();
    committed(execute(
        &store,
        delete_draft_mutation_staging_page(&store, &storage, target_page.key()),
    ));
    assert!(flight.resolve(&store).is_err());
    assert_eq!(flight.committed_receipt(), Some(&receipt));
    assert!(flight.into_prepared().is_err());
}
