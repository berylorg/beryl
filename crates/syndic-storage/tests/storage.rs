use serde_json::json;
use syndic_storage::*;
use tempfile::TempDir;

fn open_store() -> Result<(TempDir, SyndicStore)> {
    let dir = tempfile::tempdir().expect("temp dir should be created");
    let store = SyndicStore::open(dir.path(), StoreOpenOptions::default())?;
    Ok((dir, store))
}

fn source() -> ExternalSourceMetadata {
    ExternalSourceMetadata {
        provider: "codex-app-server".to_string(),
        runtime_target: Some("host-windows".to_string()),
        external_thread_id: Some("cas-thread".to_string()),
        external_turn_id: Some("cas-turn".to_string()),
        external_item_id: None,
        external_event_id: None,
    }
}

fn source_for_thread(external_thread_id: &str) -> ExternalSourceMetadata {
    ExternalSourceMetadata {
        external_thread_id: Some(external_thread_id.to_string()),
        ..source()
    }
}

fn conversation(view_id: &ThreadViewId, revision: ProviderRevision) -> ConversationRecord {
    ConversationRecord {
        id: ConversationId::from("conversation"),
        view_id: view_id.clone(),
        parent_view_id: None,
        branch_source_turn_id: None,
        title: Some("Captured transcript".to_string()),
        created_at_ms: 1,
        updated_at_ms: 2,
        current_revision: revision,
        source: Some(source()),
        history_state: HistoryState::Complete,
    }
}

fn conversation_with_source(
    id: &str,
    view_id: &ThreadViewId,
    revision: ProviderRevision,
    source: ExternalSourceMetadata,
) -> ConversationRecord {
    ConversationRecord {
        id: ConversationId::from(id),
        view_id: view_id.clone(),
        parent_view_id: None,
        branch_source_turn_id: None,
        title: None,
        created_at_ms: 1,
        updated_at_ms: 2,
        current_revision: revision,
        source: Some(source),
        history_state: HistoryState::Complete,
    }
}

fn turn(view_id: &ThreadViewId, status: TurnStatus) -> TurnRecord {
    TurnRecord {
        id: TurnId::from("turn-1"),
        conversation_id: ConversationId::from("conversation"),
        view_id: view_id.clone(),
        parent_turn_id: None,
        kind: TurnKind::User,
        status,
        source: Some(source()),
        created_at_ms: 3,
        started_at_ms: Some(4),
        completed_at_ms: None,
        terminal_error: None,
        projection_revision: ProviderRevision(1),
    }
}

fn item(turn_id: &TurnId, event_id: &SourceEventId) -> CanonicalItemRecord {
    CanonicalItemRecord {
        id: ItemId::from("item-1"),
        turn_id: turn_id.clone(),
        source_event_id: event_id.clone(),
        kind: CanonicalItemKind::AssistantMessage,
        visibility: CanonicalItemVisibility::Transcript,
        source: Some(source()),
        payload: json!({ "text": "assistant" }),
    }
}

fn provenance(
    view_id: &ThreadViewId,
    position: u64,
    turn_id: &TurnId,
    item_id: &ItemId,
    projection_id: &ProjectionRecordId,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(turn_id.clone()),
        item_id: Some(item_id.clone()),
        source_event_id: Some(SourceEventId::from("event-0")),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(ByteRange::new(position, position + 1)),
        resource_range: None,
        copy_source_range: Some(ByteRange::new(position, position + 1)),
    }
}

fn projection(
    view_id: &ThreadViewId,
    turn_id: &TurnId,
    item_id: &ItemId,
    position: u64,
    revision: ProviderRevision,
) -> ProjectionRecord {
    let projection_id = ProjectionRecordId::from(format!("projection-{position}"));
    ProjectionRecord {
        id: projection_id.clone(),
        view_id: view_id.clone(),
        turn_id: turn_id.clone(),
        item_id: item_id.clone(),
        revision,
        kind: ProjectionRecordKind::TextChunk,
        status: ProjectionStatus::Current,
        payload: ProjectionPayload::Text {
            text: format!("text {position}"),
        },
        provenance: provenance(view_id, position, turn_id, item_id, &projection_id),
    }
}

fn view_record(
    view_id: &ThreadViewId,
    turn_id: &TurnId,
    item_id: &ItemId,
    position: u64,
) -> TranscriptViewRecord {
    let projection_id = ProjectionRecordId::from(format!("projection-{position}"));
    TranscriptViewRecord {
        id: TranscriptViewRecordId::from(format!("view-record-{position}")),
        view_id: view_id.clone(),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantFinalAnswer,
        provenance: provenance(view_id, position, turn_id, item_id, &projection_id),
    }
}

fn source_event(sequence: u64) -> SourceEventRecord {
    SourceEventRecord {
        id: SourceEventId::from(format!("event-{sequence}")),
        turn_id: TurnId::from("turn-1"),
        sequence,
        captured_at_ms: 10 + sequence,
        source: source(),
        visibility: SourceEventVisibility::TranscriptVisible,
        payload: SourceEventPayload {
            kind: "agentMessageDelta".to_string(),
            body: json!({ "delta": format!("chunk-{sequence}"), "tokenUsage": sequence }),
        },
    }
}

#[test]
fn write_batch_persists_records_and_cursor_pages_are_bounded() -> Result<()> {
    let (dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view");
    let revision = ProviderRevision(5);
    let event_id = SourceEventId::from("event-0");
    let turn_id = TurnId::from("turn-1");
    let item_id = ItemId::from("item-1");

    let mut batch = SyndicWriteBatch::new()
        .put_conversation(conversation(&view_id, revision))
        .put_turn(turn(&view_id, TurnStatus::Completed))
        .put_source_event(source_event(0))
        .put_item(item(&turn_id, &event_id));

    for position in [30, 10, 20] {
        batch = batch
            .put_projection(projection(&view_id, &turn_id, &item_id, position, revision))
            .put_view_record(view_record(&view_id, &turn_id, &item_id, position));
    }

    store.commit(batch)?;
    drop(store);

    let store = SyndicStore::open(dir.path(), StoreOpenOptions::default())?;
    assert_eq!(
        store
            .conversation(&ConversationId::from("conversation"))?
            .expect("conversation should persist")
            .current_revision,
        revision
    );

    let first = store.read_transcript_page(
        &view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        2,
        Some(revision),
    )?;
    assert_eq!(positions(&first), vec![10, 20]);
    assert!(first.at_start);
    assert!(!first.at_end);
    assert!(first.previous_cursor.is_none());
    let next_cursor = first.next_cursor.expect("first page should continue");

    let second = store.read_transcript_page(
        &view_id,
        TranscriptPageAnchor::Cursor(next_cursor),
        TranscriptPageDirection::Forward,
        2,
        Some(revision),
    )?;
    assert_eq!(positions(&second), vec![30]);
    assert!(!second.at_start);
    assert!(second.at_end);

    let tail = store.read_transcript_page(
        &view_id,
        TranscriptPageAnchor::End,
        TranscriptPageDirection::Backward,
        2,
        None,
    )?;
    assert_eq!(positions(&tail), vec![20, 30]);
    assert!(!tail.at_start);
    assert!(tail.at_end);

    Ok(())
}

#[test]
fn remove_view_record_detaches_selected_path_without_deleting_projection() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-detach");
    let turn_id = TurnId::from("turn-1");
    let item_id = ItemId::from("item-1");
    let event_id = SourceEventId::from("event-0");
    let initial_revision = ProviderRevision(1);
    let detached_revision = ProviderRevision(2);

    let mut batch = SyndicWriteBatch::new()
        .put_conversation(conversation(&view_id, initial_revision))
        .put_turn(turn(&view_id, TurnStatus::Completed))
        .put_source_event(source_event(0))
        .put_item(item(&turn_id, &event_id));
    for position in [10, 20, 30] {
        batch = batch
            .put_projection(projection(
                &view_id,
                &turn_id,
                &item_id,
                position,
                initial_revision,
            ))
            .put_view_record(view_record(&view_id, &turn_id, &item_id, position));
    }
    store.commit(batch)?;

    store.commit(
        SyndicWriteBatch::new()
            .put_conversation(conversation(&view_id, detached_revision))
            .remove_view_record(
                view_id.clone(),
                TranscriptViewPosition(20),
                TranscriptViewRecordId::from("view-record-20"),
            ),
    )?;

    let page = store.read_transcript_page(
        &view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        10,
        Some(detached_revision),
    )?;
    assert_eq!(positions(&page), vec![10, 30]);
    assert!(
        store
            .projection(&ProjectionRecordId::from("projection-20"))?
            .is_some()
    );

    Ok(())
}

#[test]
fn conversation_view_lookup_returns_history_state_for_provider_reads() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-history-state");
    let mut record = conversation(&view_id, ProviderRevision(14));
    record.history_state = HistoryState::Incomplete {
        reason: HistoryIncompleteReason::StreamLost,
        detail: Some("foreground stream disconnected".to_string()),
    };

    store.commit(SyndicWriteBatch::new().put_conversation(record))?;

    let found = store
        .conversation_by_view(&view_id)?
        .expect("conversation should be indexed by view id");
    assert_eq!(found.current_revision, ProviderRevision(14));
    assert_eq!(
        found.history_state,
        HistoryState::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            detail: Some("foreground stream disconnected".to_string())
        }
    );
    assert!(
        store
            .conversation_by_view(&ThreadViewId::from("uncaptured-view"))?
            .is_none()
    );

    Ok(())
}

#[test]
fn conversation_view_summary_reads_bounded_history_facts() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-summary");
    let parent_view_id = ThreadViewId::from("view-parent");
    let revision = ProviderRevision(7);
    let turn_id = TurnId::from("turn-1");
    let event_id = SourceEventId::from("event-0");
    let item_id = ItemId::from("item-1");
    let mut conversation = conversation(&view_id, revision);
    conversation.parent_view_id = Some(parent_view_id.clone());
    conversation.branch_source_turn_id = Some(turn_id.clone());
    conversation.title = Some(" Summary title ".to_string());
    conversation.updated_at_ms = 44;

    let mut batch = SyndicWriteBatch::new()
        .put_conversation(conversation)
        .put_turn(turn(&view_id, TurnStatus::Completed))
        .put_source_event(source_event(0))
        .put_item(item(&turn_id, &event_id));
    for position in [10, 20] {
        batch = batch
            .put_projection(projection(&view_id, &turn_id, &item_id, position, revision))
            .put_view_record(view_record(&view_id, &turn_id, &item_id, position));
    }
    store.commit(
        batch.put_cas_projection_binding(CasProjectionBindingRecord {
            id: CasProjectionBindingId::from("binding-summary"),
            view_id: view_id.clone(),
            binding_revision: 3,
            selected_path_revision: revision,
            selected_path_digest: Some("summary-digest".to_string()),
            established_at_ms: 45,
            status: CasProjectionBindingStatus::Valid {
                runtime_target: "host-windows".to_string(),
                cas_thread_id: "cas-thread-summary".to_string(),
                lineage_proof: "summary-proof".to_string(),
            },
        }),
    )?;

    let summary = store
        .conversation_view_summary(&view_id)?
        .expect("summary should exist");
    assert_eq!(
        summary.conversation_id,
        ConversationId::from("conversation")
    );
    assert_eq!(summary.view_id, view_id);
    assert_eq!(summary.updated_at_ms, 44);
    assert_eq!(summary.current_revision, revision);
    assert_eq!(summary.history_state, HistoryState::Complete);
    assert_eq!(
        summary.title_candidates,
        vec![ConversationTitleCandidate {
            title: "Summary title".to_string(),
            source: ConversationTitleCandidateSource::ConversationRecord
        }]
    );
    assert_eq!(
        summary.branch,
        Some(ConversationViewBranchSummary {
            parent_view_id,
            source_turn_id: Some(turn_id.clone())
        })
    );
    let latest = summary
        .latest_transcript_record
        .expect("summary should include latest transcript record");
    assert_eq!(latest.position, TranscriptViewPosition(20));
    assert_eq!(latest.turn_id, Some(turn_id));
    assert_eq!(latest.item_id, Some(item_id));
    assert_eq!(
        summary
            .cas_projection_binding
            .expect("summary should include binding")
            .binding_revision,
        3
    );

    let summaries =
        store.conversation_view_summaries(&[ThreadViewId::from("missing"), view_id.clone()], 2)?;
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].view_id, view_id);
    assert!(matches!(
        store.conversation_view_summaries(&[ThreadViewId::from("a"), ThreadViewId::from("b")], 1),
        Err(StorageError::LimitExceeded {
            requested: 2,
            max: 1
        })
    ));

    Ok(())
}

#[test]
fn cas_projection_binding_by_view_keeps_latest_binding_revision() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-binding-index");
    store.commit(
        SyndicWriteBatch::new().put_conversation(conversation(&view_id, ProviderRevision(1))),
    )?;

    let newest = CasProjectionBindingRecord {
        id: CasProjectionBindingId::from("binding-newest"),
        view_id: view_id.clone(),
        binding_revision: 3,
        selected_path_revision: ProviderRevision(1),
        selected_path_digest: Some("newest".to_string()),
        established_at_ms: 30,
        status: CasProjectionBindingStatus::Valid {
            runtime_target: "host-windows".to_string(),
            cas_thread_id: "cas-thread-newest".to_string(),
            lineage_proof: "newest-proof".to_string(),
        },
    };
    let stale_late_write = CasProjectionBindingRecord {
        id: CasProjectionBindingId::from("binding-stale-late-write"),
        view_id: view_id.clone(),
        binding_revision: 2,
        selected_path_revision: ProviderRevision(1),
        selected_path_digest: Some("stale".to_string()),
        established_at_ms: 20,
        status: CasProjectionBindingStatus::Stale {
            old_cas_thread_id: Some("cas-thread-old".to_string()),
            reason: "older write arrived after newer binding".to_string(),
        },
    };

    store.commit(SyndicWriteBatch::new().put_cas_projection_binding(newest))?;
    store.commit(SyndicWriteBatch::new().put_cas_projection_binding(stale_late_write))?;

    let indexed = store
        .cas_projection_binding_by_view(&view_id)?
        .expect("latest binding should be indexed by view");
    assert_eq!(indexed.id, CasProjectionBindingId::from("binding-newest"));
    assert_eq!(indexed.binding_revision, 3);

    Ok(())
}

fn positions(page: &TranscriptPage) -> Vec<u64> {
    page.records
        .iter()
        .map(|record| record.position.0)
        .collect()
}

#[test]
fn conversation_source_thread_lookup_and_event_sequence_resume() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-source-index");
    let conversation = conversation_with_source(
        "conversation-source-index",
        &view_id,
        ProviderRevision(1),
        source_for_thread("cas-thread-indexed"),
    );
    store.commit(
        SyndicWriteBatch::new()
            .put_conversation(conversation)
            .put_turn(turn(&view_id, TurnStatus::Running))
            .put_source_event(source_event(0))
            .put_source_event(source_event(1)),
    )?;

    let found = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-indexed",
        )?
        .expect("external CAS thread should resolve to the Syndic conversation");
    assert_eq!(found.id, ConversationId::from("conversation-source-index"));
    assert_eq!(found.view_id, view_id);
    assert_eq!(
        store.next_source_event_sequence(&TurnId::from("turn-1"))?,
        2
    );
    assert!(
        store
            .conversation_by_external_thread(
                "codex-app-server",
                Some("host-windows"),
                "cas-thread-missing",
            )?
            .is_none()
    );

    Ok(())
}

#[test]
fn source_events_are_monotonic_idempotent_and_secret_checked() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view");
    store.commit(
        SyndicWriteBatch::new()
            .put_conversation(conversation(&view_id, ProviderRevision(1)))
            .put_turn(turn(&view_id, TurnStatus::Running)),
    )?;

    let first = source_event(0);
    let summary = store.commit(SyndicWriteBatch::new().put_source_event(first.clone()))?;
    assert_eq!(summary.idempotent_source_events, 0);

    let duplicate = store.commit(SyndicWriteBatch::new().put_source_event(first.clone()))?;
    assert_eq!(duplicate.idempotent_source_events, 1);

    let mut conflicting_duplicate = first.clone();
    conflicting_duplicate.payload.body = json!({ "delta": "changed" });
    assert!(matches!(
        store.commit(SyndicWriteBatch::new().put_source_event(conflicting_duplicate)),
        Err(StorageError::SourceEventConflict { .. })
    ));

    assert!(matches!(
        store.commit(SyndicWriteBatch::new().put_source_event(source_event(2))),
        Err(StorageError::SourceEventSequence {
            expected: 1,
            received: 2,
            ..
        })
    ));

    store.commit(SyndicWriteBatch::new().put_source_event(source_event(1)))?;
    let page = store.read_source_events(&TurnId::from("turn-1"), 0, 1)?;
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].sequence, 0);
    assert_eq!(page.next_sequence, Some(1));
    assert!(!page.at_end);

    let mut secret_event = source_event(2);
    secret_event.payload.body = json!({ "access_token": "do-not-store" });
    assert!(matches!(
        store.commit(SyndicWriteBatch::new().put_source_event(secret_event)),
        Err(StorageError::SecretLikeField { .. })
    ));

    Ok(())
}

#[test]
fn incomplete_turns_recovery_markers_and_stale_bindings_are_explicit() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-incomplete");
    let mut conversation = conversation(&view_id, ProviderRevision(9));
    conversation.history_state = HistoryState::Incomplete {
        reason: HistoryIncompleteReason::StreamLost,
        detail: Some("foreground stream disconnected".to_string()),
    };
    let incomplete_turn = turn(
        &view_id,
        TurnStatus::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            detail: Some("terminal event not observed".to_string()),
        },
    );
    let marker = RecoveryMarkerRecord {
        id: RecoveryMarkerId::from("marker-1"),
        kind: RecoveryMarkerKind::SourceIngestionInterrupted,
        view_id: Some(view_id.clone()),
        turn_id: Some(TurnId::from("turn-1")),
        created_at_ms: 99,
        detail: Some("resume with explicit incomplete state".to_string()),
    };
    let binding = CasProjectionBindingRecord {
        id: CasProjectionBindingId::from("binding-1"),
        view_id: view_id.clone(),
        binding_revision: 3,
        selected_path_revision: ProviderRevision(9),
        selected_path_digest: Some("digest-before-edit".to_string()),
        established_at_ms: 100,
        status: CasProjectionBindingStatus::Stale {
            old_cas_thread_id: Some("cas-thread".to_string()),
            reason: "ancestor edited".to_string(),
        },
    };

    store.commit(
        SyndicWriteBatch::new()
            .put_conversation(conversation)
            .put_turn(incomplete_turn)
            .put_recovery_marker(marker)
            .put_cas_projection_binding(binding),
    )?;

    let saved_conversation = store
        .conversation(&ConversationId::from("conversation"))?
        .expect("conversation should exist");
    assert!(matches!(
        saved_conversation.history_state,
        HistoryState::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            ..
        }
    ));

    let saved_turn = store
        .turn(&TurnId::from("turn-1"))?
        .expect("turn should exist");
    assert!(matches!(
        saved_turn.status,
        TurnStatus::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            ..
        }
    ));

    let markers = store.list_recovery_markers(8)?;
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].id, RecoveryMarkerId::from("marker-1"));

    let saved_binding = store
        .cas_projection_binding(&CasProjectionBindingId::from("binding-1"))?
        .expect("binding should exist");
    assert!(matches!(
        saved_binding.status,
        CasProjectionBindingStatus::Stale { .. }
    ));

    store.commit(
        SyndicWriteBatch::new().clear_recovery_marker(RecoveryMarkerId::from("marker-1")),
    )?;
    assert!(store.list_recovery_markers(8)?.is_empty());

    Ok(())
}

#[test]
fn resource_range_reads_validate_bounds_and_limits() -> Result<()> {
    let (_dir, store) = open_store()?;
    let resource_id = ResourceId::from("resource-code");
    let bytes = b"0123456789abcdef".to_vec();
    store.commit(SyndicWriteBatch::new().put_resource(ResourceRecord {
        metadata: ResourceMetadataRecord {
            id: resource_id.clone(),
            revision: ProviderRevision(4),
            kind: ResourceKind::Code,
            state: ResourceState::Ready,
            media_type: Some("text/plain".to_string()),
            byte_len: 0,
            digest: Some("sha256:test".to_string()),
            line_count: Some(1),
            row_count: None,
            column_count: None,
            preview_range: Some(ByteRange::new(0, 4)),
        },
        bytes: bytes.clone(),
    }))?;

    let metadata = store
        .resource_metadata(&resource_id)?
        .expect("metadata should exist");
    assert_eq!(metadata.byte_len, bytes.len() as u64);

    let range = store.read_resource_range(
        &resource_id,
        ByteRange::new(2, 6),
        Some(ProviderRevision(4)),
    )?;
    assert_eq!(range.bytes, b"2345".to_vec());
    assert!(!range.complete);

    assert!(matches!(
        store.read_resource_range(&resource_id, ByteRange::new(1, 0), None),
        Err(StorageError::ResourceRangeOutOfBounds { .. })
    ));
    assert!(matches!(
        store.read_resource_range(&resource_id, ByteRange::new(0, 17), None),
        Err(StorageError::ResourceRangeOutOfBounds { .. })
    ));
    assert!(matches!(
        store.read_resource_range(
            &resource_id,
            ByteRange::new(0, 4),
            Some(ProviderRevision(3))
        ),
        Err(StorageError::StaleRecordRevision { .. })
    ));

    let huge_resource_id = ResourceId::from("resource-huge");
    let huge_bytes = vec![7; MAX_RESOURCE_RANGE_BYTES as usize + 2];
    store.commit(SyndicWriteBatch::new().put_resource(ResourceRecord {
        metadata: ResourceMetadataRecord {
            id: huge_resource_id.clone(),
            revision: ProviderRevision(1),
            kind: ResourceKind::Attachment,
            state: ResourceState::Ready,
            media_type: Some("application/octet-stream".to_string()),
            byte_len: 0,
            digest: None,
            line_count: None,
            row_count: None,
            column_count: None,
            preview_range: None,
        },
        bytes: huge_bytes,
    }))?;
    assert!(matches!(
        store.read_resource_range(
            &huge_resource_id,
            ByteRange::new(0, MAX_RESOURCE_RANGE_BYTES + 1),
            None,
        ),
        Err(StorageError::ResourceRangeTooLarge { .. })
    ));

    Ok(())
}

#[test]
fn stale_view_revisions_reject_projection_reads() -> Result<()> {
    let (_dir, store) = open_store()?;
    let view_id = ThreadViewId::from("view-stale");
    let revision = ProviderRevision(12);
    let turn_id = TurnId::from("turn-1");
    let item_id = ItemId::from("item-1");
    let mut projection_record = projection(&view_id, &turn_id, &item_id, 1, revision);
    projection_record.status = ProjectionStatus::Stale {
        reason: HistoryIncompleteReason::ProjectionStale,
        detail: Some("rebuild pending".to_string()),
    };
    let projection_id = projection_record.id.clone();

    store.commit(
        SyndicWriteBatch::new()
            .put_conversation(conversation(&view_id, revision))
            .put_turn(turn(&view_id, TurnStatus::Completed))
            .put_projection(projection_record),
    )?;

    let records = store.read_projection_records(&view_id, &[projection_id], Some(revision))?;
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].status,
        ProjectionStatus::Stale {
            reason: HistoryIncompleteReason::ProjectionStale,
            ..
        }
    ));
    assert!(matches!(
        store.read_projection_records(
            &view_id,
            &[ProjectionRecordId::from("projection-1")],
            Some(ProviderRevision(11)),
        ),
        Err(StorageError::StaleRevision { .. })
    ));

    Ok(())
}
