#[cfg(feature = "test-faults")]
fn prepare_uncommitted_window(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> (
    syndic_storage::PreparedDraftPieceStagingWindowV1,
    syndic_storage::DraftMutationStagingProgressReceiptKeyV1,
    syndic_storage::DraftPieceBuildProgressReceiptReferenceV1,
    syndic_storage::DraftPieceSettlementKeyV1,
    DraftMutationStagingIdentityV1,
) {
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([operation; 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, session), session)
        .unwrap();
    let mut active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    for _ in 0..2 {
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let page = prepare_one_page(
            *storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::SourcePosition(point(0)),
        );
        active = page.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
        ));
    }
    let replacement = DraftPieceReplacementV1::new(
        point(0),
        point(0),
        vec![DraftPieceV1::Text("x".to_owned())],
    );
    let chain = draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let page = prepare_one_page(
        *storage,
        &head,
        &active,
        DraftMutationStagingPageItemV1::Proposal(replacement),
    );
    active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &active,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                DraftLogicalExtentV1::new(1, 1),
                point(0),
                point(0),
                point(0),
                chain,
            ),
        )
        .unwrap();
    active = finish.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &active)
        .unwrap();
    committed(execute(
        store,
        storage
            .transfer_draft_mutation_staging_to_builder(storage.revision(store).unwrap(), transfer),
    ));
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(store, identity)
        .unwrap()
    else {
        panic!("replay fixture lost builder custody");
    };
    let window = storage
        .prepare_next_durable_draft_piece_window(
            store,
            identity,
            build,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )
        .unwrap()
        .unwrap();
    let page_key = syndic_storage::DraftMutationStagingPageKeyV1::new(
        identity,
        window.lane(),
        window.first_page_ordinal(),
    )
    .unwrap();
    let page = storage
        .draft_mutation_staging_page(store, page_key)
        .unwrap()
        .unwrap();
    let page_receipt = syndic_storage::DraftMutationStagingProgressReceiptKeyV1::new(
        identity,
        page.transition_ordinal(),
    )
    .unwrap();
    let settlement_key = syndic_storage::DraftPieceSettlementKeyV1::new(
        identity.draft_id(),
        identity.session_id(),
        identity.operation_id().as_piece_operation(),
    );
    (
        window,
        page_receipt,
        build,
        settlement_key,
        identity,
    )
}

#[cfg(feature = "test-faults")]
#[test]
fn staging_window_source_and_target_replay_require_page_and_build_receipts() {
    for case in 0_u8..8 {
        let (_home, store, storage, thread) = fixture(&format!("window-replay-{case}"), 100 + case);
        let current = current(storage, &store, thread);
        let session = open_session(storage, &store, &current, 110 + case, 120 + case);
        let (window, page_receipt, source_build_receipt, settlement_key, _) =
            prepare_uncommitted_window(&storage, &store, &session, 125 + case);
        let target_build_receipt = window.target_endpoint().key();
        if case >= 4 {
            committed(execute(
                &store,
                storage.stage_next_durable_draft_piece_window(
                    storage.revision(&store).unwrap(),
                    window.clone(),
                ),
            ));
        }
        let deletion = match case {
            0 | 4 => syndic_storage::test_faults::delete_draft_mutation_staging_receipt(
                &store,
                storage,
                page_receipt,
            ),
            1 | 5 => {
                syndic_storage::test_faults::inject_draft_mutation_staging_receipt_digest_corruption(
                    &store,
                    storage,
                    page_receipt,
                )
            }
            2 => syndic_storage::test_faults::delete_draft_piece_build_progress_receipt(
                &store,
                storage,
                source_build_receipt.key(),
            ),
            3 | 7 => syndic_storage::test_faults::inject_draft_piece_progress_receipt_corruption(
                &store,
                storage,
                settlement_key,
                syndic_storage::test_faults::DraftPieceProgressReceiptCorruption::StateMismatch,
            ),
            6 => syndic_storage::test_faults::delete_draft_piece_build_progress_receipt(
                &store,
                storage,
                target_build_receipt,
            ),
            _ => unreachable!(),
        };
        committed(execute(&store, deletion));
        assert!(matches!(
            execute(
                &store,
                storage.stage_next_durable_draft_piece_window(
                    storage.revision(&store).unwrap(),
                    window,
                ),
            ),
            CommandOutcome::NotCommitted { .. }
        ));
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn staging_window_acquisition_and_replay_require_build_receipt_predecessor() {
    for case in 0_u8..4 {
        let (home, store, storage, thread) =
            fixture(&format!("window-predecessor-{case}"), 140 + case);
        let current = current(storage, &store, thread);
        let session = open_session(storage, &store, &current, 150 + case, 160 + case);
        let (window, _, _source_endpoint, settlement_key, identity) =
            prepare_uncommitted_window(&storage, &store, &session, 170 + case);
        committed(execute(
            &store,
            storage.stage_next_durable_draft_piece_window(
                storage.revision(&store).unwrap(),
                window.clone(),
            ),
        ));
        let corruption = if case % 2 == 0 {
            syndic_storage::test_faults::DraftPieceProgressReceiptCorruption::DeletePrevious
        } else {
            syndic_storage::test_faults::DraftPieceProgressReceiptCorruption::PreviousStateMismatch
        };
        committed(execute(
            &store,
            syndic_storage::test_faults::inject_draft_piece_progress_receipt_corruption(
                &store,
                storage,
                settlement_key,
                corruption,
            ),
        ));
        if case < 2 {
            assert!(
                storage
                    .prepare_next_durable_draft_piece_window(
                        &store,
                        identity,
                        window.target_endpoint(),
                        DraftPieceDurableBuildWindowLimitsV1::maximum(),
                    )
                    .is_err()
            );
        } else {
            assert!(matches!(
                execute(
                    &store,
                    storage.stage_next_durable_draft_piece_window(
                        storage.revision(&store).unwrap(),
                        window.clone(),
                    ),
                ),
                CommandOutcome::NotCommitted { .. }
            ));
        }
        drop(store);
        let mut reopened =
            HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        if case < 2 {
            assert!(
                reopened_storage
                    .prepare_next_durable_draft_piece_window(
                        &reopened,
                        identity,
                        window.target_endpoint(),
                        DraftPieceDurableBuildWindowLimitsV1::maximum(),
                    )
                    .is_err()
            );
        } else {
            assert!(matches!(
                execute(
                    &reopened,
                    reopened_storage.stage_next_durable_draft_piece_window(
                        reopened_storage.revision(&reopened).unwrap(),
                        window,
                    ),
                ),
                CommandOutcome::NotCommitted { .. }
            ));
        }
    }
}
