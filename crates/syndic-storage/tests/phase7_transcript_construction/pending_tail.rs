use super::*;

#[test]
fn pending_tail_stays_out_of_public_entries_until_its_frontier_is_finalized() {
    let home = TestHome::new("phase7-transcript-complete-tail-gate");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage);
    let authored = format!(
        "```text\n{}\n```\n",
        "x".repeat(MARKDOWN_CODE_INLINE_MAX_BYTES + 1)
    );
    let pending = submit_text(
        &store,
        storage,
        thread,
        &authored,
        draft_id(3),
        SyndicItemId::from_bytes([40; 16]),
        timestamp(2),
        timestamp(3),
    );
    let initial_item_generation = project_item(&store, storage, pending.item);
    let initial_item_set = storage
        .item_projection_set(&store, pending.item, initial_item_generation, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    let initial_projection_ids = item_projection_ids(&store, storage, pending.item);
    let initial_resource_ids = projection_resource_ids(&store, storage, &initial_projection_ids);
    assert_eq!(initial_item_set.resource_count(), 1);
    assert_eq!(initial_resource_ids.len(), 1);

    let (pending_generation, _) = start_transcript_build(&store, storage, thread);
    let pending_build = finish_transcript_build(&store, storage, thread, pending_generation);
    assert_eq!(pending_build.entry_count(), 0);
    assert!(!pending_build.history_complete());
    let pending_path = path_turns(&store, storage, thread, pending_generation);
    assert_eq!(pending_path.len(), 1);
    assert_eq!(pending_path[0].finalized_item_count(), 0);
    assert!(transcript_entries(&store, storage, thread, pending_generation).is_empty());
    let pending_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(pending_head.entry_count(), 0);
    assert_eq!(pending_head.lifecycle(), ProjectionLifecycle::Current);
    assert!(
        !storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );

    let source = establish_turn(&store, storage, thread, pending.turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    let local_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    let submitted_content = local_item
        .presentation_content()
        .expect("submitted user item has sealed composer content");
    let provider_item = ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 {
            content: submitted_content,
        },
    });
    let cas_item_id = CasItemId::new("phase13-correlated-user").unwrap();
    admit_item_frame(
        &store,
        storage,
        thread,
        pending.turn,
        pending.item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item_id.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(timestamp(4).unix_millis()),
                item: provider_item.clone(),
            },
        ),
        timestamp(4),
    );
    let started_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        started_item.revision(),
        initial_item_set
            .source_item_revision()
            .checked_next()
            .unwrap()
    );
    let stale_item_head = storage
        .item_projection_head(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stale_item_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(stale_item_head.generation(), initial_item_generation);
    let stale_transcript_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_transcript_head.lifecycle(),
        ProjectionLifecycle::Stale
    );

    let started_item_generation = project_item(&store, storage, pending.item);
    let started_item_set = storage
        .item_projection_set(&store, pending.item, started_item_generation, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_ne!(started_item_generation, initial_item_generation);
    assert_eq!(started_item_set.source(), initial_item_set.source());
    assert_eq!(
        started_item_set.stable_digest(),
        initial_item_set.stable_digest()
    );
    assert_eq!(started_item_set.digest(), initial_item_set.digest());
    assert_eq!(
        started_item_set.resume_checkpoint(),
        initial_item_set.resume_checkpoint()
    );
    assert_eq!(
        item_projection_ids(&store, storage, pending.item),
        initial_projection_ids
    );
    assert_eq!(
        projection_resource_ids(&store, storage, &initial_projection_ids),
        initial_resource_ids
    );

    let (started_transcript_generation, _) = start_transcript_build(&store, storage, thread);
    assert_ne!(started_transcript_generation, pending_generation);
    let started_transcript =
        finish_transcript_build(&store, storage, thread, started_transcript_generation);
    assert_eq!(started_transcript.entry_count(), 0);
    admit_item_frame(
        &store,
        storage,
        thread,
        pending.turn,
        pending.item,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item_id,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(timestamp(4).unix_millis()),
                item: provider_item,
            },
        ),
        timestamp(4),
    );
    let published_completed_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        published_completed_item.revision(),
        started_item.revision().checked_next().unwrap()
    );
    let stale_item_head = storage
        .item_projection_head(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stale_item_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(stale_item_head.generation(), started_item_generation);
    let stale_transcript_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_transcript_head.lifecycle(),
        ProjectionLifecycle::Stale
    );
    assert_ne!(
        stale_transcript_head.generation(),
        started_transcript_generation
    );
    admit(
        &store,
        storage,
        thread,
        pending.turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(5),
    );
    let state = storage
        .turn_state(&store, pending.turn, point_limit())
        .unwrap()
        .unwrap();
    let mut rejected = HomeCommand::new(store.home_revision().unwrap());
    rejected
        .add(storage.finalize_next_turn_item(
            storage.revision(&store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                pending.turn,
                state.revision(),
                TurnItemOrdinal::FIRST,
                pending.item,
                timestamp(6),
            ),
        ))
        .unwrap();
    let error = store.execute(rejected).unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CanonicalFinalizationConflict
    ));

    let state = storage
        .turn_state(&store, pending.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.freeze_next_turn_item(
            storage.revision(&store).unwrap(),
            FreezeNextTurnItem::new(
                thread,
                pending.turn,
                state.revision(),
                TurnItemOrdinal::FIRST,
                pending.item,
                timestamp(6),
            ),
        ),
    );
    let completed_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        completed_item.revision(),
        published_completed_item.revision().checked_next().unwrap()
    );

    let completed_item_generation = project_item(&store, storage, pending.item);
    let completed_item_set = storage
        .item_projection_set(
            &store,
            pending.item,
            completed_item_generation,
            point_limit(),
        )
        .unwrap()
        .unwrap()
        .clone();
    assert_ne!(completed_item_generation, started_item_generation);
    assert_eq!(
        completed_item_set.source_item_revision(),
        completed_item.revision()
    );
    assert_eq!(completed_item_set.source(), initial_item_set.source());
    assert_eq!(
        completed_item_set.stable_digest(),
        initial_item_set.stable_digest()
    );
    assert_eq!(completed_item_set.digest(), initial_item_set.digest());
    assert_eq!(
        completed_item_set.resume_checkpoint(),
        initial_item_set.resume_checkpoint()
    );
    assert_eq!(
        item_projection_ids(&store, storage, pending.item),
        initial_projection_ids
    );
    assert_eq!(
        projection_resource_ids(&store, storage, &initial_projection_ids),
        initial_resource_ids
    );

    let state = storage
        .turn_state(&store, pending.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.finalize_next_turn_item(
            storage.revision(&store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                pending.turn,
                state.revision(),
                TurnItemOrdinal::FIRST,
                pending.item,
                timestamp(6),
            ),
        ),
    );

    let (final_generation, _) = start_transcript_build(&store, storage, thread);
    assert_ne!(final_generation, pending_generation);
    let completed = finish_transcript_build(&store, storage, thread, final_generation);
    assert_eq!(completed.entry_count(), 1);
    assert!(completed.history_complete());
    let entries = transcript_entries(&store, storage, thread, final_generation);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].position(), TranscriptPosition::FIRST);
    assert_eq!(entries[0].item_id(), pending.item);
    assert!(
        storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );

    store.validate_registered_domains().unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    let reopened_item = reopened_storage
        .canonical_item(&reopened, pending.item, point_limit())
        .unwrap()
        .unwrap();
    let reopened_head = reopened_storage
        .item_projection_head(&reopened, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(reopened_head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(
        reopened_head.source_item_revision(),
        reopened_item.revision()
    );
    reopened.close().unwrap();
}
