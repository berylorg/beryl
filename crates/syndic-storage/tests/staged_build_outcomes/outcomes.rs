use super::*;

const MAX_RESUMES: usize = 4_096;

pub(super) fn assert_verification_budget(work: syndic_storage::StagedDraftPieceVerificationWorkV1) {
    assert_eq!(
        work.charged_encoded_value_bytes,
        work.attempted_reads * 65_536
    );
    assert!(
        work.attempted_reads <= syndic_storage::STAGED_DRAFT_PIECE_OUTCOME_MAX_READS,
        "verification exceeded its read allowance: {work:?}"
    );
    assert!(
        work.charged_encoded_value_bytes
            <= syndic_storage::STAGED_DRAFT_PIECE_OUTCOME_MAX_ENCODED_VALUE_BYTES,
        "verification exceeded its byte allowance: {work:?}"
    );
}

pub(super) fn assert_known_commit_verification(
    work: syndic_storage::StagedDraftPieceVerificationWorkV1,
) {
    assert_eq!(
        work,
        syndic_storage::StagedDraftPieceVerificationWorkV1::default(),
        "a known durable command must not invoke referenced-closure verification"
    );
}

pub(super) fn complete(
    mut flight: syndic_storage::StagedDraftPieceOutcomeFlightV1,
    store: &HomeStore,
) -> syndic_storage::StagedDraftPieceCommandCompletionV1 {
    for _ in 0..MAX_RESUMES {
        assert_verification_budget(flight.verification_work());
        match flight.into_completion() {
            Ok(completion) => {
                assert_verification_budget(completion.verification);
                return completion;
            }
            Err(next) => {
                assert!(
                    !matches!(
                        next.state(),
                        syndic_storage::StagedDraftPieceOutcomeStateV1::Unavailable
                            | syndic_storage::StagedDraftPieceOutcomeStateV1::NotCommitted
                    ),
                    "staged command unexpectedly became terminal before completion: {next:?}"
                );
                flight = next.resume(store);
            }
        }
    }
    panic!(
        "staged command did not reach a completion within its bounded retry budget: {flight:?}, cleanup failure: {:?}",
        flight.cleanup_failure()
    );
}

pub(super) fn building_endpoint(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
) -> syndic_storage::DraftPieceBuildProgressReceiptReferenceV1 {
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(store, identity)
        .unwrap()
    else {
        panic!("staged operation did not retain durable builder custody");
    };
    build
}

pub(super) fn finish_plain_staging(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    include_source_position: bool,
    replacements: Vec<DraftPieceReplacementV1>,
    extent: DraftLogicalExtentV1,
) -> DraftMutationStagingIdentityV1 {
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
    if include_source_position {
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let source = prepare_one_page(
            storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::SourcePosition(point(0)),
        );
        active = source.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), source),
        ));
    }
    let mut chain = canonical_empty_draft_piece_fragment_chain_v1();
    for replacement in replacements {
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let page = prepare_one_page(
            storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::Proposal(replacement.clone()),
        );
        active = page.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
        ));
        chain =
            draft_piece_fragment_chain_link_v1(chain, head.proposal().next_ordinal(), &replacement);
    }
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
                extent,
                point(0),
                point(0),
                point(0),
                chain,
            ),
        )
        .unwrap();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    identity
}

fn finish_many_plain_staging(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacements: &[DraftPieceReplacementV1],
    extent: DraftLogicalExtentV1,
) -> DraftMutationStagingIdentityV1 {
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
    for pages in replacements.chunks(256) {
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let first_cursor = head.proposal().next_cursor();
        let inputs = pages
            .iter()
            .enumerate()
            .map(|(offset, replacement)| {
                let cursor = first_cursor + offset as u64;
                DraftMutationStagingPageInputV1::new(
                    DraftMutationStagingLaneV1::Proposal,
                    cursor,
                    cursor + 1,
                    1,
                    65_536,
                    Box::new([DraftMutationStagingPageItemV1::Proposal(
                        replacement.clone(),
                    )]),
                )
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let page = storage
            .prepare_draft_mutation_staging_page_batch(&head, &active, inputs)
            .unwrap();
        active = page.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
        ));
    }
    let chain = replacements.iter().enumerate().fold(
        canonical_empty_draft_piece_fragment_chain_v1(),
        |chain, (offset, replacement)| {
            draft_piece_fragment_chain_link_v1(chain, offset as u64 + 1, replacement)
        },
    );
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
                extent,
                point(0),
                point(0),
                point(0),
                chain,
            ),
        )
        .unwrap();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    identity
}

pub(super) fn transfer(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
) -> syndic_storage::DraftPieceBuildProgressReceiptReferenceV1 {
    let finished = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let prepared = storage
        .prepare_staged_draft_piece_transfer(store, identity, finished.receipt())
        .unwrap();
    assert_eq!(
        prepared.kind(),
        syndic_storage::StagedDraftPieceCommandKindV1::Transfer
    );
    let completion = complete(prepared.submit(store), store);
    assert_known_commit_verification(completion.verification);
    assert!(matches!(
        completion.result,
        DraftPieceReconciledCommandV1::Pending(_)
    ));
    building_endpoint(storage, store, identity)
}

pub(super) fn stage_windows(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    limits: DraftPieceDurableBuildWindowLimitsV1,
) -> usize {
    let mut endpoint = building_endpoint(storage, store, identity);
    let mut windows = 0;
    loop {
        let Some(prepared) = storage
            .prepare_staged_draft_piece_window(store, identity, endpoint, limits)
            .unwrap()
        else {
            return windows;
        };
        assert_eq!(
            prepared.kind(),
            syndic_storage::StagedDraftPieceCommandKindV1::Window
        );
        let completion = complete(prepared.submit(store), store);
        assert_known_commit_verification(completion.verification);
        assert!(matches!(
            completion.result,
            DraftPieceReconciledCommandV1::Pending(_)
        ));
        assert_verification_budget(completion.verification);
        windows += 1;
        assert!(windows < MAX_RESUMES, "staging windows did not converge");
        endpoint = building_endpoint(storage, store, identity);
    }
}

pub(super) fn advance_to_terminal(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
) -> syndic_storage::DraftPieceBuildProgressReceiptReferenceV1 {
    let mut endpoint = building_endpoint(storage, store, identity);
    let mut advances = 0;
    loop {
        let Some(prepared) = storage
            .prepare_staged_draft_piece_advance(store, identity, endpoint)
            .unwrap()
        else {
            return endpoint;
        };
        assert_eq!(
            prepared.kind(),
            syndic_storage::StagedDraftPieceCommandKindV1::Advance
        );
        let completion = complete(prepared.submit(store), store);
        assert_known_commit_verification(completion.verification);
        assert!(matches!(
            completion.result,
            DraftPieceReconciledCommandV1::Pending(_)
        ));
        assert_verification_budget(completion.verification);
        advances += 1;
        assert!(advances < MAX_RESUMES, "build advances did not converge");
        endpoint = building_endpoint(storage, store, identity);
    }
}

pub(super) fn finish_admitted_staging(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    proof: DraftMarkerLabelReadinessProofV1,
    replacements: &[DraftPieceReplacementV1],
) -> DraftMutationStagingIdentityV1 {
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes(*proof.owner().operation_id().as_bytes()),
    );
    let begin = storage
        .prepare_draft_mutation_staging_marker_begin(begin_input(identity, session), session, proof)
        .unwrap();
    let mut active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    for replacement in replacements {
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let page = prepare_one_page(
            storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::Proposal(replacement.clone()),
        );
        active = page.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
        ));
    }
    let chain = replacements.iter().enumerate().fold(
        canonical_empty_draft_piece_fragment_chain_v1(),
        |chain, (offset, replacement)| {
            draft_piece_fragment_chain_link_v1(chain, offset as u64 + 1, replacement)
        },
    );
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
                session.logical_extent(),
                DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
                DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
                DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
                chain,
            ),
        )
        .unwrap();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    identity
}

#[test]
fn post_finish_commands_stage_source_then_proposal_and_settle_actual_roots() {
    let (_home, store, storage, thread) = fixture("outcome-source-window", 1);
    let before = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &before, 2, 3);
    let identity = finish_plain_staging(
        &storage,
        &store,
        &session,
        4,
        true,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("source-window".to_owned())],
        )],
        DraftLogicalExtentV1::new(13, 1),
    );
    let endpoint = transfer(&storage, &store, identity);
    let source_window = storage
        .prepare_staged_draft_piece_window(
            &store,
            identity,
            endpoint,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let completion = complete(source_window.submit(&store), &store);
    assert_known_commit_verification(completion.verification);
    assert!(matches!(
        completion.result,
        DraftPieceReconciledCommandV1::Pending(_)
    ));
    assert_verification_budget(completion.verification);
    let DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(build)) =
        completion.result
    else {
        panic!("source window did not retain an open build");
    };
    let source_advanced = build.durable_continuation().unwrap();
    assert_eq!(source_advanced.source().next_ordinal(), 2);
    assert_eq!(source_advanced.proposal().next_ordinal(), 1);
    assert_eq!(
        stage_windows(
            &storage,
            &store,
            identity,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        ),
        1
    );
    let endpoint = advance_to_terminal(&storage, &store, identity);
    let prepared = storage
        .prepare_staged_draft_piece_terminal(
            &store,
            identity,
            endpoint,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Settle,
        )
        .unwrap();
    let completion = complete(prepared.submit(&store), &store);
    assert_known_commit_verification(completion.verification);
    assert!(matches!(
        completion.result,
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(_))
    ));
    let settled = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert!(settled.active_operation().is_none());
    assert_ne!(settled.newest_root(), session.newest_root());
    assert_ne!(settled.newest_history(), session.newest_history());
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                settled.newest_root(),
                DraftPieceTextDemandV1::Forward(0),
                65_536,
            )
            .unwrap()
            .bytes(),
        b"source-window"
    );
    assert_eq!(current(&storage, &store, thread), before);
}

#[test]
fn stale_and_foreign_build_endpoints_are_rejected_before_submission() {
    let (_home, store, storage, thread) = fixture("outcome-stale-endpoint", 20);
    let current = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &current, 21, 22);
    let identity = finish_plain_staging(
        &storage,
        &store,
        &session,
        23,
        false,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("one".to_owned())],
        )],
        DraftLogicalExtentV1::new(3, 1),
    );
    let first_endpoint = transfer(&storage, &store, identity);
    assert_eq!(
        stage_windows(
            &storage,
            &store,
            identity,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        ),
        1
    );
    assert!(matches!(
        storage.prepare_staged_draft_piece_window(
            &store,
            identity,
            first_endpoint,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        ),
        Err(syndic_storage::StagedDraftPiecePreparationErrorV1::StaleEndpoint)
    ));
    let endpoint = advance_to_terminal(&storage, &store, identity);
    let terminal = storage
        .prepare_staged_draft_piece_terminal(
            &store,
            identity,
            endpoint,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Cancel,
        )
        .unwrap();
    let cancelled = complete(terminal.submit(&store), &store);
    assert!(matches!(
        cancelled.result,
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Cancelled(_))
    ));
    let next_session = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert!(next_session.active_operation().is_none());
    let second = finish_plain_staging(
        &storage,
        &store,
        &next_session,
        24,
        false,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("two".to_owned())],
        )],
        DraftLogicalExtentV1::new(3, 1),
    );
    let second_endpoint = transfer(&storage, &store, second);
    assert_ne!(first_endpoint, second_endpoint);
    assert!(matches!(
        storage.prepare_staged_draft_piece_window(
            &store,
            second,
            first_endpoint,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        ),
        Err(syndic_storage::StagedDraftPiecePreparationErrorV1::StaleEndpoint)
    ));
    let terminal = storage
        .prepare_staged_draft_piece_terminal(
            &store,
            second,
            second_endpoint,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Cancel,
        )
        .unwrap();
    let cancelled = complete(terminal.submit(&store), &store);
    assert!(matches!(
        cancelled.result,
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Cancelled(_))
    ));
}

#[test]
fn terminal_elections_preserve_the_candidate_without_a_second_edit_submission() {
    let (_home, store, storage, thread) = fixture("outcome-terminal-matrix", 30);
    let initial = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &initial, 31, 32);
    for (operation, election) in [
        (
            33,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Cancel,
        ),
        (
            34,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Reject(
                DraftPieceRejectedReasonV1::InvalidGapWitness,
            ),
        ),
        (
            35,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Error(
                DraftPieceErrorReasonV1::ResourceLimit,
            ),
        ),
    ] {
        let original_root = session.newest_root();
        let original_history = session.newest_history();
        let identity = finish_plain_staging(
            &storage,
            &store,
            &session,
            operation,
            false,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("unused".to_owned())],
            )],
            DraftLogicalExtentV1::new(6, 1),
        );
        transfer(&storage, &store, identity);
        stage_windows(
            &storage,
            &store,
            identity,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        );
        let endpoint = advance_to_terminal(&storage, &store, identity);
        let prepared = storage
            .prepare_staged_draft_piece_terminal(&store, identity, endpoint, election)
            .unwrap();
        let completion = complete(prepared.submit(&store), &store);
        assert_known_commit_verification(completion.verification);
        assert!(matches!(
            completion.result,
            DraftPieceReconciledCommandV1::Terminal(_)
        ));
        session = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert!(session.active_operation().is_none());
        assert_eq!(session.newest_root(), original_root);
        assert_eq!(session.newest_history(), original_history);
    }
    assert_eq!(current(&storage, &store, thread), initial);
}

#[test]
fn fresh_and_accepted_marker_targets_preserve_committed_cleanup_custody_when_home_fails() {
    let faults = beryl_home_store::test_faults::FaultController::new();
    let mut fixture = AcceptedFixture::with_faults("outcome-fresh-cleanup", 40, faults.clone());
    let admission = owner(&fixture.session, 41);
    ingest(
        &fixture,
        admission,
        42,
        1,
        false,
        vec![fresh(43, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    ingest(
        &fixture,
        admission,
        43,
        2,
        true,
        vec![fixture.association(44, fixture.thread)],
        || Some(fixture.factory()),
    );
    let proof = assign(&fixture, admission, 45, 2);
    let fresh_marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([43; 16]),
        1,
        label(&fixture, &proof, 43),
        fixture.asset_id,
    );
    let accepted_marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([44; 16]),
        2,
        label(&fixture, &proof, 44),
        fixture.asset_id,
    );
    let before_all = point(0);
    let after_all = point(0);
    let replacements = [
        DraftPieceReplacementV1::new(
            before_all,
            before_all,
            vec![DraftPieceV1::Marker(fresh_marker)],
        )
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                0,
                fresh_marker,
                DraftPieceMarkerEffectChargesV1::for_marker(fresh_marker),
            ),
        )),
        DraftPieceReplacementV1::new(
            after_all,
            after_all,
            vec![DraftPieceV1::Marker(accepted_marker)],
        )
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                0,
                accepted_marker,
                DraftPieceMarkerEffectChargesV1::for_marker(accepted_marker),
            ),
        )),
    ];
    let identity = finish_admitted_staging(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        proof,
        &replacements,
    );
    transfer(&fixture.storage, &fixture.store, identity);
    assert_eq!(
        stage_windows(
            &fixture.storage,
            &fixture.store,
            identity,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        ),
        2
    );
    let endpoint = advance_to_terminal(&fixture.storage, &fixture.store, identity);
    let prepared = fixture
        .storage
        .prepare_staged_draft_piece_terminal(
            &fixture.store,
            identity,
            endpoint,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Settle,
        )
        .unwrap();
    let mut flight = prepared.submit(&fixture.store);
    for _ in 0..MAX_RESUMES {
        if flight.state() == syndic_storage::StagedDraftPieceOutcomeStateV1::CleanupPending {
            break;
        }
        assert_ne!(
            flight.state(),
            syndic_storage::StagedDraftPieceOutcomeStateV1::Complete,
            "admitted settlement skipped its required cleanup state"
        );
        assert_verification_budget(flight.verification_work());
        flight = flight.resume(&fixture.store);
    }
    assert_eq!(
        flight.state(),
        syndic_storage::StagedDraftPieceOutcomeStateV1::CleanupPending
    );
    assert_eq!(
        flight.classification(),
        syndic_storage::StagedDraftPieceDurableClassificationV1::Committed
    );
    assert!(matches!(
        flight.result(),
        Some(DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Committed(_)
        ))
    ));
    faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeCommit);
    let flight = flight.resume(&fixture.store);
    assert_eq!(
        flight.state(),
        syndic_storage::StagedDraftPieceOutcomeStateV1::CleanupPending
    );
    assert_eq!(
        flight.classification(),
        syndic_storage::StagedDraftPieceDurableClassificationV1::Committed
    );
    assert!(flight.cleanup_failure().is_some());
    assert!(matches!(
        flight.result(),
        Some(DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Committed(_)
        ))
    ));
    let committed_result = flight.result().unwrap().clone();
    let flight = flight.resume(&fixture.store);
    assert_eq!(
        flight.state(),
        syndic_storage::StagedDraftPieceOutcomeStateV1::CleanupPending
    );
    assert_eq!(flight.result(), Some(&committed_result));
    assert!(flight.cleanup_failure().is_some());
    assert_known_commit_verification(flight.verification_work());
    let recovery = fixture.store.recover_same_home().unwrap();
    fixture.storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    fixture.store = recovery.publish();
    let flight = flight.resume(&fixture.store);
    assert_eq!(
        flight.state(),
        syndic_storage::StagedDraftPieceOutcomeStateV1::Unavailable
    );
    assert_eq!(
        flight.classification(),
        syndic_storage::StagedDraftPieceDurableClassificationV1::Committed
    );
    assert_eq!(flight.result(), Some(&committed_result));
    assert!(flight.cleanup_failure().is_some());
    let settled = active_session(
        &fixture.storage,
        &fixture.store,
        fixture.session.draft_id(),
        fixture.session.session_id(),
    );
    assert!(settled.active_operation().is_none());
    assert_eq!(settled.newest_root().summary().marker_count(), 2);
    for marker in [fresh_marker, accepted_marker] {
        assert!(
            fixture
                .storage
                .draft_marker_identity(&fixture.store, settled.newest_root(), marker.marker_id())
                .unwrap()
                .is_some()
        );
        assert!(
            fixture
                .storage
                .validate_draft_marker_location(
                    &fixture.store,
                    settled.newest_root(),
                    DraftPieceMarkerAtV1::new(0, marker),
                )
                .unwrap()
        );
    }
    let reclaimed = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, admission, &[])
        .unwrap();
    assert!(reclaimed.head().is_some());
    assert!(reclaimed.receipt().is_none());
}

#[test]
fn more_than_256_fragments_continue_across_windows_with_bounded_outcome_work() {
    const FRAGMENT_COUNT: usize = 257;
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("outcome-many-fragments", 60, faults.clone());
    let current = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &current, 61, 62);
    let session = complete_staged(
        &storage,
        &store,
        &session,
        63,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".repeat(FRAGMENT_COUNT))],
        ),
        DraftLogicalExtentV1::new(FRAGMENT_COUNT as u64, 1),
    );
    let replacements = (0..FRAGMENT_COUNT)
        .map(|offset| {
            DraftPieceReplacementV1::new(
                point(offset as u64),
                point(offset as u64 + 1),
                vec![DraftPieceV1::Text("b".to_owned())],
            )
        })
        .collect::<Vec<_>>();
    let identity = finish_many_plain_staging(
        &storage,
        &store,
        &session,
        64,
        &replacements,
        DraftLogicalExtentV1::new(FRAGMENT_COUNT as u64, 1),
    );
    transfer(&storage, &store, identity);
    let windows = stage_windows(
        &storage,
        &store,
        identity,
        DraftPieceDurableBuildWindowLimitsV1::new(256, 256, 65_536).unwrap(),
    );
    assert_eq!(
        windows, 2,
        "one operation must cross its durable window boundary"
    );
    let endpoint = advance_to_terminal(&storage, &store, identity);
    let terminal = storage
        .prepare_staged_draft_piece_terminal(
            &store,
            identity,
            endpoint,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Settle,
        )
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let flight = terminal.submit(&store);
    assert!(flight.has_reconciliation_custody());
    syndic_storage::test_faults::reset_home_store_syndic_point_acquisition_count();
    let completion = complete(flight, &store);
    assert!(completion.original_failure.is_some());
    assert!(completion.verification.attempted_reads > 0);
    assert_eq!(
        syndic_storage::test_faults::home_store_syndic_point_acquisition_count(),
        completion.verification.attempted_reads as u64
    );
    assert!(matches!(
        completion.result,
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(_))
    ));
    assert_verification_budget(completion.verification);
    let settled = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert!(settled.active_operation().is_none());
    assert_ne!(settled.newest_root(), session.newest_root());
    assert_ne!(settled.newest_history(), session.newest_history());
    let mut bytes = Vec::new();
    let mut offset = 0;
    while offset < settled.newest_root().summary().logical_utf8_bytes() {
        let page = storage
            .draft_piece_text_demand(
                &store,
                settled.newest_root(),
                DraftPieceTextDemandV1::Forward(offset),
                65_536,
            )
            .unwrap();
        assert_eq!(page.start(), offset);
        assert!(page.end() > offset);
        offset = page.end();
        bytes.extend_from_slice(page.bytes());
    }
    assert_eq!(bytes, "b".repeat(FRAGMENT_COUNT).as_bytes());
}
