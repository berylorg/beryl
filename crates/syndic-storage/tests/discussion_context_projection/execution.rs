use super::*;
use beryl_model::{CasItemId, CasTurnId, SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId};

pub(super) fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

fn publish_seed(store: &HomeStore, storage: &SyndicStorage) {
    let (basis, seed, native_count) = native_seed(store, storage);
    let lineage = seed
        .completed_lineage(timestamp(100), generation())
        .unwrap();
    committed(store.execute_current(storage.current_publish_valid_binding(
        PublishValidBinding::from_native(
            basis,
            request(store, storage).execution().clone(),
            CasThreadId::new("context-initial-target").unwrap(),
            native_count,
            lineage,
        ),
    )));
}

pub(super) fn complete_pending(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    user: SyndicItemId,
    answer: Option<(SyndicItemId, &str)>,
    time: u64,
) {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("usable prepared target");
    };
    let selected = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .selected_path();
    let turn = selected.tail().unwrap();
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes(*turn.as_bytes());
    let classified = storage
        .prepare_context_binding_cancellable(store, thread, usable, &|| false)
        .unwrap();
    support::enroll_fixture_activity(store, storage, thread, turn);
    committed(
        store.execute_current(
            storage.current_activate_binding(
                ActivateBinding::new(
                    thread,
                    current.binding().revision(),
                    gate.revision(),
                    state.revision(),
                    selected,
                    snapshot,
                    turn,
                    generation(),
                    timestamp(time),
                )
                .with_context_classification(classified),
            ),
        ),
    );
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = CasTurnId::new(format!("context-turn-{turn}")).unwrap();
    committed(
        store.execute_current(
            storage.current_publish_active_cas_turn(PublishActiveCasTurn::new(
                thread,
                current.binding().revision(),
                gate.revision(),
                snapshot,
                usable.cas_thread_id().clone(),
                cas_turn.clone(),
                timestamp(time + 1),
            )),
        ),
    );
    let source = CasTurnSource::new(usable.cas_thread_id().clone(), cas_turn);
    support::exact_cas::admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(time + 2),
    );
    support::exact_cas::correlate_user_item(
        store,
        storage.clone(),
        thread,
        turn,
        user,
        &source,
        timestamp(time + 3),
    );
    if let Some((item, text)) = answer {
        let answer = ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
            text: ProviderTextV1::inline(text),
            phase: Some(ProviderMessagePhaseV1::FinalAnswer),
            memory_citation: None,
        });
        support::exact_cas::admit_started_then_completed_item(
            store,
            storage.clone(),
            thread,
            turn,
            item,
            &source,
            CasItemId::new(format!("context-answer-{item}")).unwrap(),
            answer.clone(),
            answer,
            timestamp(time + 4),
            timestamp(time + 5),
        );
    }
    support::exact_cas::admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(time + 6),
    );
    support::exact_cas::converge_and_release_terminal_history(store, storage.clone(), thread, turn);
}

pub(super) fn create_discussion_at_answer(
    store: &HomeStore,
    storage: &SyndicStorage,
    parent: SyndicThreadId,
    source_turn: SyndicTurnId,
    source_item: SyndicItemId,
    text: &str,
    child: u8,
    draft: u8,
) {
    let thread = storage
        .thread(store, parent, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, parent, point_limit())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            parent,
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == source_item)
        .unwrap();
    let source = storage
        .prepare_discussion_source(
            store,
            DiscussionContextSource::new(
                parent,
                source_turn,
                source_item,
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, text.len() as u64).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new(text).unwrap(),
        )
        .unwrap();
    let prepared = storage
        .prepare_discussion_creation(
            store,
            source,
            CreateDiscussion::new(
                id(child),
                draft_id(draft),
                timestamp(200),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        )
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    committed(store.execute(command));
}

#[test]
fn same_discussion_continuation_uses_historical_source_beneath_the_new_unbound_head() {
    let (_home, store, storage) = seeded("context-same-discussion-continuation");
    publish_seed(&store, &storage);
    complete_pending(
        &store,
        &storage,
        id(210),
        SyndicItemId::from_bytes([213; 16]),
        None,
        110,
    );
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(210),
        draft_id(245),
        SyndicItemId::from_bytes([246; 16]),
        "continued ordinary input",
        timestamp(200),
    );
    let request = request(&store, &storage);
    let NativeProjectionPlan::Resume { basis, source } = storage
        .prepare_native_projection(&store, &request, point_limit())
        .unwrap()
    else {
        panic!("same discussion must resume its exact original native target");
    };
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        current.binding().state(),
        BindingState::Unbound { .. }
    ));
    assert_eq!(
        current.binding().revision(),
        basis.expected_binding_revision()
    );
    assert_ne!(source.binding_revision(), basis.expected_binding_revision());
    assert!(!basis.context().unwrap().needs_seed());
    assert_eq!(basis.context().unwrap().summary().count(), 1);
    let original = source.binding().lineage();
    assert_eq!(original.recovered_completed_at(), Some(timestamp(100)));
    assert_eq!(
        original.recovered_injection_generation(),
        Some(generation())
    );
    committed(store.execute_current(storage.current_publish_valid_binding(
        PublishValidBinding::from_native(
            basis,
            request.execution().clone(),
            source.binding().cas_thread_id().clone(),
            source.binding().native_turn_count(),
            original,
        ),
    )));
    complete_pending(
        &store,
        &storage,
        id(210),
        SyndicItemId::from_bytes([246; 16]),
        None,
        210,
    );
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("completed continuation retains a usable original target");
    };
    assert_eq!(usable.lineage(), original);
    let selected = storage
        .thread(&store, id(210), point_limit())
        .unwrap()
        .unwrap()
        .selected_path();
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(
                id(210),
                selected,
                Some(1_000_000),
            )
            .with_tool_profile(request.tool_profile()),
        )
        .unwrap()
    else {
        panic!("completed ordinary inputs remain exactly replayable");
    };
    let items = replay(&store, &storage, projection, 37);
    assert_eq!(items.len(), 5);
    assert_eq!(items[3].1, "discuss this exactly");
    assert_eq!(items[4].1, "continued ordinary input");
    assert_eq!(
        items
            .iter()
            .filter(|item| item.1.starts_with("[Beryl discussion context v1]\n"))
            .count(),
        1,
    );
    store.close().unwrap();
}

#[test]
fn nested_selected_crossings_replay_outer_then_inner_context_once_each() {
    let (_home, store, storage) = seeded("context-nested-crossings");
    publish_seed(&store, &storage);
    let first_turn = request(&store, &storage).selected_path().tail().unwrap();
    let answer = SyndicItemId::from_bytes([226; 16]);
    complete_pending(
        &store,
        &storage,
        id(210),
        SyndicItemId::from_bytes([213; 16]),
        Some((answer, "nested answer")),
        110,
    );
    create_discussion_at_answer(
        &store,
        &storage,
        id(210),
        first_turn,
        answer,
        "nested answer",
        227,
        228,
    );
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(227),
        draft_id(229),
        SyndicItemId::from_bytes([230; 16]),
        "nested user",
        timestamp(210),
    );
    let target = request_for(&store, &storage, id(227));
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                id(227),
                target.selected_path(),
                Some(1_000_000),
            )
            .with_tool_profile(target.tool_profile()),
        )
        .unwrap()
    else {
        panic!("nested complete replay");
    };
    assert_eq!(projection.context().unwrap().summary().count(), 2);
    let items = replay(&store, &storage, projection, 53);
    assert_eq!(items.len(), 6);
    assert_eq!(items[0].1, "source question");
    assert_eq!(items[1].1, "assistant");
    assert!(
        items[2]
            .1
            .ends_with("\n\nassistant\n[/Beryl discussion context v1]")
    );
    assert_eq!(items[3].1, "discuss this exactly");
    assert_eq!(items[4].1, "nested answer");
    assert!(
        items[5]
            .1
            .ends_with("\n\nnested answer\n[/Beryl discussion context v1]")
    );
    assert!(!items.iter().any(|item| item.1 == "nested user"));
    store.close().unwrap();
}

#[test]
fn ordinary_descendants_with_no_context_owner_cannot_erase_snapshot_inherited_context() {
    let (_home, store, storage) = seeded("context-ordinary-descendant-proof");
    publish_seed(&store, &storage);
    complete_pending(
        &store,
        &storage,
        id(210),
        SyndicItemId::from_bytes([213; 16]),
        None,
        110,
    );
    native_fixtures::seed_child_at_tail(&store, &storage, id(210), id(237), draft_id(238));
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(237),
        draft_id(239),
        SyndicItemId::from_bytes([240; 16]),
        "ordinary inherited input",
        timestamp(200),
    );
    let request = request_for(&store, &storage, id(237));
    let NativeProjectionPlan::Fork {
        basis,
        native_turn_count,
        ..
    } = storage
        .prepare_native_projection(&store, &request, point_limit())
        .unwrap()
    else {
        panic!("ordinary descendant inherits native context");
    };
    let context = basis.context().unwrap();
    assert!(!context.needs_seed());
    let proof = context
        .complete_native(NativeCasLineage::Fork, generation(), None)
        .unwrap();
    assert_eq!(proof.recovered_completed_at(), Some(timestamp(100)));
    committed(store.execute_current(storage.current_publish_valid_binding(
        PublishValidBinding::from_native(
            basis,
            request.execution().clone(),
            CasThreadId::new("first-ordinary-descendant").unwrap(),
            native_turn_count,
            proof,
        ),
    )));
    complete_pending(
        &store,
        &storage,
        id(237),
        SyndicItemId::from_bytes([240; 16]),
        None,
        210,
    );
    native_fixtures::seed_child_at_tail(&store, &storage, id(237), id(241), draft_id(242));
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(241),
        draft_id(243),
        SyndicItemId::from_bytes([244; 16]),
        "second ordinary input",
        timestamp(300),
    );
    let request = request_for(&store, &storage, id(241));
    assert!(
        storage
            .thread(&store, id(241), point_limit())
            .unwrap()
            .unwrap()
            .context_owner_id()
            .is_none()
    );
    let NativeProjectionPlan::Fork {
        basis,
        native_turn_count,
        ..
    } = storage
        .prepare_native_projection(&store, &request, point_limit())
        .unwrap()
    else {
        panic!("second descendant inherits context through snapshot");
    };
    let parent = storage
        .turn(
            &store,
            basis.represented_prefix().tail().unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert!(
        storage
            .thread(&store, parent.origin_thread_id(), point_limit())
            .unwrap()
            .unwrap()
            .context_owner_id()
            .is_none()
    );
    for lineage in [
        CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap(),
        CasLineageProof::recovered(
            RecoveredInjectionProof::new(
                RecoveryProjectionVersion::V1,
                basis.represented_prefix(),
                beryl_model::RecoveryItemSequenceDigest::from_bytes([33; 32]),
                RecoveryItemCount::new(1).unwrap(),
                RecoveryUtf8ByteCount::new(1).unwrap(),
                timestamp(301),
                generation(),
            )
            .unwrap(),
        ),
    ] {
        let raw = PublishValidBinding::new(
            id(241),
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution().clone(),
            CasThreadId::new("second-ordinary-descendant").unwrap(),
            basis.represented_prefix(),
            native_turn_count,
            request.tool_profile(),
            lineage,
        );
        assert!(matches!(
            store.execute_current(storage.current_publish_valid_binding(raw)),
            CommandOutcome::NotCommitted { .. }
        ));
    }
    let inherited = basis
        .context()
        .unwrap()
        .complete_native(NativeCasLineage::Fork, generation(), None)
        .unwrap();
    committed(store.execute_current(storage.current_publish_valid_binding(
        PublishValidBinding::from_native(
            basis,
            request.execution().clone(),
            CasThreadId::new("second-ordinary-descendant").unwrap(),
            native_turn_count,
            inherited,
        ),
    )));
    let bytes = test_faults::encode_lineage_fixture(inherited);
    assert_eq!(bytes.len(), 310);
    assert_eq!(&bytes[..3], &[2, 1, 1]);
    assert_eq!(
        test_faults::decode_lineage_fixture(&bytes).unwrap(),
        inherited
    );
    store.close().unwrap();
}
