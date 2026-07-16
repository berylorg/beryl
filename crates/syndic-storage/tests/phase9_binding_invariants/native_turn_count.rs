use super::*;

fn provider_operation_seed(
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    turn: SyndicTurnId,
) -> FixtureBatch {
    let thread_revision = beryl_model::ThreadRevision::new(1).unwrap();
    let digest = root_turn_chain_digest(turn);
    let mut records = thread_records_with_activity(thread, draft, Some(turn), digest, timestamp(2));
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            timestamp(2),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
    ]);
    records.extend(item_free_transcript_build_records(
        thread,
        thread_revision,
        &[(turn, digest, TurnLifecycle::Interrupted, 1, timestamp(2))],
    ));
    batch(records)
}

#[test]
fn provider_operation_depth_does_not_seed_fork_or_resume_native_counts() {
    let home = TestHome::new("phase10-provider-depth-native-count");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(110);
    let draft = draft_id(111);
    let turn = SyndicTurnId::from_bytes([112; 16]);
    commit(
        &store,
        storage,
        provider_operation_seed(thread, draft, turn),
    );

    let selected = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .selected_path();
    let represented =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());
    let cas_thread = CasThreadId::new("phase10-provider-fork").unwrap();
    publish_valid(
        &store,
        storage,
        valid_request_with_count(
            &store,
            storage,
            thread,
            selected,
            cas_thread.clone(),
            represented,
            CasNativeTurnCount::ZERO,
            CasLineageProof::native(NativeCasLineage::Fork, represented).unwrap(),
        ),
    );
    let forked = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(forked) = forked.binding().state() else {
        panic!("forked provider projection is not valid");
    };
    assert_eq!(forked.native_turn_count(), CasNativeTurnCount::ZERO);
    assert_eq!(
        storage
            .turn(&store, turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .depth(),
        TurnDepth::FIRST
    );

    let forged_resume = valid_request_with_count(
        &store,
        storage,
        thread,
        selected,
        cas_thread.clone(),
        represented,
        CasNativeTurnCount::new(1),
        CasLineageProof::native(NativeCasLineage::Resume, represented).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), forged_resume),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingPathConflict
    ));

    publish_valid(
        &store,
        storage,
        valid_request_with_count(
            &store,
            storage,
            thread,
            selected,
            cas_thread,
            represented,
            CasNativeTurnCount::ZERO,
            CasLineageProof::native(NativeCasLineage::Resume, represented).unwrap(),
        ),
    );
    let resumed = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(resumed) = resumed.binding().state() else {
        panic!("resumed provider projection is not valid");
    };
    assert_eq!(resumed.native_turn_count(), CasNativeTurnCount::ZERO);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
