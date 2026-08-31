use super::*;

pub(super) fn ready_proof(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
    command_seed: u8,
    associations: Vec<syndic_storage::DraftMarkerReadinessSourceAssociationV1>,
) -> syndic_storage::DraftMarkerLabelReadinessProofV1 {
    if associations.is_empty() {
        let mut attempt = storage
            .prepare_draft_marker_label_readiness_page(
                store,
                syndic_storage::DraftMarkerLabelReadinessPageRequestV1::new(
                    admission,
                    syndic_storage::DraftMarkerAdmissionCommandIdV1::from_bytes([command_seed; 16]),
                    NonZeroU64::MIN,
                    true,
                    DraftMarkerLabelReadinessDispositionV1::Reuse,
                    Box::new([]),
                    None,
                ),
            )
            .unwrap();
        let receipt = store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        assert!(matches!(
            storage.submit_draft_marker_label_readiness_page(
                store,
                attempt.into_submission_flight(store, receipt).unwrap(),
            ),
            syndic_storage::DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
        ));
        let flight = storage
            .prepare_draft_marker_label_assignment(
                store,
                admission,
                syndic_storage::DraftMarkerAdmissionCommandIdV1::from_bytes(
                    [command_seed.wrapping_add(1); 16],
                ),
            )
            .unwrap();
        return match storage.submit_draft_marker_label_assignment(store, flight) {
            syndic_storage::DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
            _ => panic!("empty readiness did not issue a proof"),
        };
    }
    let count = u8::try_from(associations.len()).unwrap();
    for _ in 0..count {
        let mut attempt = storage
            .prepare_draft_marker_label_readiness_page(
                store,
                syndic_storage::DraftMarkerLabelReadinessPageRequestV1::new(
                    admission,
                    syndic_storage::DraftMarkerAdmissionCommandIdV1::from_bytes([command_seed; 16]),
                    NonZeroU64::MIN,
                    true,
                    DraftMarkerLabelReadinessDispositionV1::Reuse,
                    associations.clone().into_boxed_slice(),
                    None,
                ),
            )
            .unwrap();
        let receipt = store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        let flight = attempt.into_submission_flight(store, receipt).unwrap();
        assert!(matches!(
            storage.submit_draft_marker_label_readiness_page(store, flight),
            syndic_storage::DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
        ));
    }
    for command in 1..=count {
        let flight = storage
            .prepare_draft_marker_label_assignment(
                store,
                admission,
                syndic_storage::DraftMarkerAdmissionCommandIdV1::from_bytes(
                    [command_seed.wrapping_add(command); 16],
                ),
            )
            .unwrap();
        match storage.submit_draft_marker_label_assignment(store, flight) {
            syndic_storage::DraftMarkerLabelAssignmentOutcomeV1::Advanced { .. } => {}
            syndic_storage::DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. }
                if command == count =>
            {
                return proof;
            }
            _ => panic!("readiness assignment did not make exact progress"),
        }
    }
    panic!("readiness assignment did not issue a proof")
}

pub(super) fn fixture_with_history_budget(
    name: &str,
    seed: u8,
    budget: u64,
    history_policy_revision: u64,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
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
                        "C:\\syndic-phase225-history-budget",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(budget, history_policy_revision)
                    .unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

pub(super) fn marked_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> (
    syndic_storage::DraftEditorCandidateSessionV1,
    DraftPieceMarkerV1,
) {
    let durable = current(storage, store, thread);
    let session = open_session(storage, store, &durable, seed, seed.wrapping_add(1));
    let session = complete_staged(
        storage,
        store,
        &session,
        seed.wrapping_add(2),
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker = marker(seed.wrapping_add(3), 1, 7);
    let admission = owner(&session, seed.wrapping_add(5));
    let proof = storage
        .seed_draft_marker_writer_ready_target_for_test(store, &session, admission, marker)
        .unwrap();
    let session = complete_admitted_marker_edit(
        storage,
        store,
        &session,
        admission,
        proof,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
    );
    storage
        .release_settled_draft_marker_writer(store, admission)
        .unwrap();
    (session, marker)
}

pub(super) fn complete_admitted_marker_edit(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
    proof: syndic_storage::DraftMarkerLabelReadinessProofV1,
    replacement: DraftPieceReplacementV1,
) -> syndic_storage::DraftEditorCandidateSessionV1 {
    let (prepared, identity, _) =
        stage_admitted_marker_edit(storage, store, session, admission, proof, replacement);
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
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
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

pub(super) fn complete_admitted_marker_edits(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
    proof: syndic_storage::DraftMarkerLabelReadinessProofV1,
    replacements: Vec<DraftPieceReplacementV1>,
) {
    assert!(replacements.len() > 1);
    let (identity, mut active, staged) =
        begin_admitted_marker_edit(storage, store, session, admission, proof);
    let page = storage
        .prepare_draft_mutation_staging_page_batch(
            &staged,
            &active,
            replacements
                .iter()
                .enumerate()
                .map(|(index, replacement)| {
                    let cursor = staged.proposal().next_cursor() + index as u64;
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
                .into_boxed_slice(),
        )
        .unwrap();
    active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));
    let receiving = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let chain = replacements.iter().enumerate().fold(
        canonical_empty_draft_piece_fragment_chain_v1(),
        |chain, (index, replacement)| {
            draft_piece_fragment_chain_link_v1(chain, (index + 1) as u64, replacement)
        },
    );
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &receiving,
            &active,
            DraftMutationFinishInputV1::new(
                receiving.source(),
                receiving.proposal(),
                session.logical_extent(),
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
    let prepared_head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&prepared_head, &active)
        .unwrap();
    let prepared = transfer.prepared_edit().clone();
    committed(execute(
        store,
        storage
            .transfer_draft_mutation_staging_to_builder(storage.revision(store).unwrap(), transfer),
    ));
    let DraftMutationStagingStatusV1::Building {
        build: build_ref, ..
    } = storage
        .draft_mutation_staging_status(store, identity)
        .unwrap()
    else {
        panic!("admitted staging did not transfer to building")
    };
    let window = storage
        .prepare_next_durable_draft_piece_window(
            store,
            identity,
            build_ref,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )
        .unwrap()
        .unwrap();
    committed(execute(
        store,
        storage.stage_next_durable_draft_piece_window(storage.revision(store).unwrap(), window),
    ));
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
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
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
}

pub(super) fn begin_admitted_marker_edit(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
    proof: syndic_storage::DraftMarkerLabelReadinessProofV1,
) -> (
    syndic_storage::DraftMutationStagingIdentityV1,
    syndic_storage::DraftEditorCandidateSessionV1,
    syndic_storage::DraftMutationStagingHeadV1,
) {
    let identity = syndic_storage::DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes(*admission.operation_id().as_bytes()),
    );
    let begin = storage
        .prepare_draft_mutation_staging_marker_begin(begin_input(identity, session), session, proof)
        .unwrap();
    let active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    assert!(head.begin().writer_admission().is_some());
    (identity, active, head)
}

pub(super) fn snapshot(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
) -> syndic_storage::DraftMarkerAdmissionPublicationSnapshotV1 {
    storage
        .draft_marker_admission_publication_snapshot_for_test(store, admission, &[])
        .unwrap()
}

pub(super) fn stage_admitted_marker_edit(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
    proof: syndic_storage::DraftMarkerLabelReadinessProofV1,
    replacement: DraftPieceReplacementV1,
) -> (
    syndic_storage::PreparedDraftPieceEditV1,
    syndic_storage::DraftMutationStagingIdentityV1,
    Vec<syndic_storage::DraftPieceBuildFragmentV1>,
) {
    stage_admitted_marker_edit_with_extent(
        storage,
        store,
        session,
        admission,
        proof,
        replacement,
        session.logical_extent(),
    )
}

pub(super) fn stage_admitted_marker_edit_with_extent(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    admission: syndic_storage::DraftMarkerAdmissionOwnerV1,
    proof: syndic_storage::DraftMarkerLabelReadinessProofV1,
    replacement: DraftPieceReplacementV1,
    final_extent: DraftLogicalExtentV1,
) -> (
    syndic_storage::PreparedDraftPieceEditV1,
    syndic_storage::DraftMutationStagingIdentityV1,
    Vec<syndic_storage::DraftPieceBuildFragmentV1>,
) {
    let (identity, mut active, staged) =
        begin_admitted_marker_edit(storage, store, session, admission, proof);
    let page = prepare_one_page(
        storage,
        &staged,
        &active,
        syndic_storage::DraftMutationStagingPageItemV1::Proposal(replacement.clone()),
    );
    active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));
    let receiving = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let chain = draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &receiving,
            &active,
            DraftMutationFinishInputV1::new(
                receiving.source(),
                receiving.proposal(),
                final_extent,
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
    let prepared_head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&prepared_head, &active)
        .unwrap();
    let prepared = transfer.prepared_edit().clone();
    committed(execute(
        store,
        storage
            .transfer_draft_mutation_staging_to_builder(storage.revision(store).unwrap(), transfer),
    ));
    let DraftMutationStagingStatusV1::Building {
        build: build_ref, ..
    } = storage
        .draft_mutation_staging_status(store, identity)
        .unwrap()
    else {
        panic!("admitted staging did not transfer to building")
    };
    let window = storage
        .prepare_next_durable_draft_piece_window(
            store,
            identity,
            build_ref,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )
        .unwrap()
        .unwrap();
    let fragments = window.fragments_for_test().to_vec();
    committed(execute(
        store,
        storage.stage_next_durable_draft_piece_window(storage.revision(store).unwrap(), window),
    ));
    (prepared, identity, fragments)
}
