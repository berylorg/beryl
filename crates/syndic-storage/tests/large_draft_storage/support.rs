fn complete_staged_bounded(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
    final_extent: DraftLogicalExtentV1,
) -> DraftEditorCandidateSessionV1 {
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
    assert!(page.page_count() <= DRAFT_MUTATION_STAGING_BATCH_MAX_PAGES);
    assert!(page.item_count() <= DRAFT_MUTATION_STAGING_BATCH_MAX_ITEMS);
    assert!(page.encoded_page_bytes() <= DRAFT_MUTATION_STAGING_BATCH_MAX_BYTES);
    active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));

    let head = storage
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
            &head,
            &active,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
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

    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &active)
        .unwrap();
    let prepared = transfer.prepared_edit().clone();
    committed(execute(
        store,
        storage
            .transfer_draft_mutation_staging_to_builder(storage.revision(store).unwrap(), transfer),
    ));
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(store, identity)
        .unwrap()
    else {
        panic!("staging transfer did not retain builder custody");
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
    assert!(window.page_count() <= DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES);
    assert!(window.fragment_count() <= DRAFT_PIECE_BUILD_WINDOW_MAX_FRAGMENTS);
    assert!(window.inserted_utf8_bytes() <= DRAFT_PIECE_BUILD_WINDOW_MAX_INSERTED_UTF8_BYTES);
    assert!(window.acquisition_read_count() <= DRAFT_PIECE_BUILD_WINDOW_MAX_READS);
    assert!(
        window.acquisition_encoded_value_byte_budget()
            <= DRAFT_PIECE_BUILD_WINDOW_MAX_ENCODED_VALUE_BYTES
    );
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
        assert!(advance.records_read() <= DRAFT_PIECE_BUILD_WINDOW_MAX_READS as u64);
        assert!(advance.staged_record_count() <= DRAFT_PIECE_STAGE_MAX_RECORDS);
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

fn large_chunk(chunk: usize) -> String {
    let mut bytes = vec![0_u8; LARGE_CHUNK_BYTES];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = expected_byte((chunk * LARGE_CHUNK_BYTES + offset) as u64);
    }
    String::from_utf8(bytes).unwrap()
}

fn expected_byte(offset: u64) -> u8 {
    b'a' + ((offset / LARGE_CHUNK_BYTES as u64 + offset) % 26) as u8
}

fn assert_sparse_text(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    offset: u64,
    edited: Option<u64>,
) {
    let result = storage
        .draft_piece_text_demand(
            store,
            root,
            syndic_storage::DraftPieceTextDemandV1::Forward(offset),
            4096,
        )
        .unwrap();
    assert_eq!(result.start(), offset);
    assert!(!result.bytes().is_empty());
    assert!(result.bytes().len() <= 4096);
    assert!(result.bytes().len() <= DRAFT_PIECE_PAGE_MAX_BYTES);
    assert!(result.records_read() <= u64::from(DRAFT_PIECE_MAX_HEIGHT) + 2);
    for (index, byte) in result.bytes().iter().copied().enumerate() {
        let position = offset + index as u64;
        assert_eq!(
            byte,
            if edited == Some(position) {
                b'!'
            } else {
                expected_byte(position)
            }
        );
    }
}

fn adopt_history(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    direction: DraftHistoricalRootDirectionV1,
) -> DraftEditorCandidateSessionV1 {
    let intent = DraftHistoricalRootSelectionIntentV1::new(
        DraftEditorCandidateActivationBindingV1::from_head(session),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        direction,
    );
    let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
        .prepare_draft_historical_root_selection(store, intent)
        .unwrap()
    else {
        panic!("history direction unexpectedly unavailable");
    };
    committed(execute(
        store,
        storage.adopt_draft_historical_root(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

fn capture_publication_source(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: DraftEditorCandidatePublicationRequestV1,
) -> syndic_storage::CapturedDraftEditorCandidatePublicationSourceV1 {
    let head = active_session(
        storage,
        store,
        request.selector().draft_id(),
        request.session_id(),
    );
    let candidate = DraftEditorCandidateActivationBindingV1::new(
        request.selector().draft_id(),
        request.session_id(),
        head.session_generation(),
        request.candidate_generation(),
        request.candidate().root(),
        request.candidate().history(),
        request.candidate().root().summary().logical_extent(),
    );
    storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                candidate,
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap()
}

fn materialize_bounded(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    operation: u8,
) -> syndic_storage::DraftComposerMaterializationRecordV1 {
    let key = DraftComposerBuildKeyV1::new(
        root,
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([operation; 16]),
    );
    committed(execute(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    ));
    for _ in 0..16_384 {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
        {
            return mapping;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(store, key)
            .unwrap()
            .unwrap();
        assert!(prepared.records_read() <= DRAFT_COMPOSER_READ_MAX_RECORDS);
        assert!(prepared.input_payload_bytes() <= DRAFT_COMPOSER_INPUT_MAX_BYTES);
        assert!(prepared.written_record_count() <= DRAFT_COMPOSER_WRITE_MAX_RECORDS);
        assert!(prepared.resident_bytes() <= DRAFT_COMPOSER_RESIDENT_MAX_BYTES);
        committed(execute(
            store,
            storage
                .advance_draft_composer_materialization(storage.revision(store).unwrap(), prepared),
        ));
    }
    panic!("bounded composer materialization did not finish");
}
