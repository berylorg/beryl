use super::*;

pub(super) fn seed_terminal_turn_with_open_assistant(
    store: &HomeStore,
    storage: SyndicStorage,
) -> TerminalTurn {
    let thread = id(1);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft_id(2),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let user_item = SyndicItemId::from_bytes([4; 16]);
    let turn = submit_text(
        store,
        storage,
        thread,
        "original question",
        draft_id(3),
        user_item,
        timestamp(2),
        timestamp(3),
    );
    let assistant_item = SyndicItemId::from_bytes([5; 16]);
    let cas_assistant = CasItemId::new("phase7-selected-path-assistant").unwrap();
    let source = establish_turn(store, storage, thread, turn, timestamp(4));
    admit(
        store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        assistant_item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_assistant.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(5),
                item: agent_value(""),
            },
        ),
        timestamp(5),
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        assistant_item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_assistant.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline("retained answer"),
            }),
        ),
        timestamp(6),
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        assistant_item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(3).unwrap(),
            cas_assistant,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(7),
                item: agent_value("retained answer"),
            },
        ),
        timestamp(7),
    );
    admit(
        store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Complete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(8),
    );

    let fixture = TerminalTurn {
        thread,
        turn,
        user_item,
        assistant_item,
    };
    project_item(store, storage, user_item);
    finalize_item(
        store,
        storage,
        thread,
        turn,
        TurnItemOrdinal::FIRST,
        user_item,
        timestamp(8),
    );
    let head = publish_transcript(store, storage, thread);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(head.entry_count(), 1);
    fixture
}

pub(super) fn replace_with_completed_root(
    store: &HomeStore,
    storage: SyndicStorage,
    old: TerminalTurn,
) -> (SyndicTurnId, SyndicItemId) {
    let thread = storage
        .thread(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let draft = storage
        .current_draft(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let entry = transcript_entry(store, storage, old.thread, head.generation(), old.user_item);
    let selected_path = SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    );
    execute(
        store,
        storage.start_replacement_edit(
            storage.revision(store).unwrap(),
            StartReplacementEdit::new(
                old.thread,
                thread.revision(),
                draft.draft().id(),
                draft.draft().revision(),
                gate.revision(),
                old.turn,
                old.user_item,
                selected_path,
                CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
                None,
                timestamp(9),
            ),
        ),
    );

    let editing = storage
        .current_draft(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let thread = storage
        .thread(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let replacement_item = SyndicItemId::from_bytes([7; 16]);
    let submission = IdleSubmission::new(
        old.thread,
        thread.revision(),
        editing.draft().id(),
        editing.draft().revision(),
        editing.draft().content(),
        gate.revision(),
        draft_id(6),
        replacement_item,
        None,
        timestamp(10),
    );
    let replacement_turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );

    let source = establish_turn(store, storage, old.thread, replacement_turn, timestamp(11));
    admit(
        store,
        storage,
        old.thread,
        replacement_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(11),
    );
    correlate_user_item(
        store,
        storage,
        old.thread,
        replacement_turn,
        replacement_item,
        &source,
        timestamp(11),
    );
    admit(
        store,
        storage,
        old.thread,
        replacement_turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(12),
    );
    let state = storage
        .turn_state(store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.freeze_next_turn_item(
            storage.revision(store).unwrap(),
            FreezeNextTurnItem::new(
                old.thread,
                replacement_turn,
                state.revision(),
                TurnItemOrdinal::FIRST,
                replacement_item,
                timestamp(13),
            ),
        ),
    );
    project_item(store, storage, replacement_item);
    finalize_item(
        store,
        storage,
        old.thread,
        replacement_turn,
        TurnItemOrdinal::FIRST,
        replacement_item,
        timestamp(13),
    );
    let head = publish_transcript(store, storage, old.thread);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(head.committed_tail(), Some(replacement_turn));
    assert_eq!(head.entry_count(), 1);

    let replacement = storage
        .turn(store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(replacement.parent(), ConversationParent::Root);
    (replacement_turn, replacement_item)
}

pub(super) fn head(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> TranscriptViewHeadRecord {
    storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .clone()
}

pub(super) fn summary(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> HistorySummaryRecord {
    storage
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .clone()
}

pub(super) fn assistant_content_lifecycle(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: TerminalTurn,
) -> ContentLifecycle {
    let item = storage
        .canonical_item(store, fixture.assistant_item, point_limit())
        .unwrap()
        .unwrap();
    storage
        .content_manifest(
            store,
            item.provider_content()
                .expect("assistant item has canonical content")
                .id(),
            point_limit(),
        )
        .unwrap()
        .unwrap()
        .lifecycle()
}
