use super::*;

#[test]
fn awaiting_and_prepared_sources_reopen_with_candidate_proofs_and_fresh_handles() {
    let f = MediaHome::new("generated-source-reopen");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    f.frame(
        21,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\durable.png")),
        7,
    );
    let awaiting = f.proof(20);
    let completed = f.proof(21);
    let asset = f.prepare(21);
    let stale_unavailable = f
        .storage
        .unavailable_generated_media(awaiting.clone(), Reason::LostSourceAuthority, timestamp(8))
        .unwrap();
    let stale_publication = f
        .storage
        .publish_generated_media(completed.clone(), asset, "image/png", None, timestamp(8))
        .unwrap();
    let original_generation = f.store.generation_identity().unwrap();
    let original_counter = f.store.health().generation().unwrap();
    let resource = awaiting.resource_id();
    let prepared_resource = completed.resource_id();
    f.store.close().unwrap();
    let mut candidate = open(f.home.path());
    let state = BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register_with_schema_validation(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    {
        let access = publication.recovery_access().unwrap();
        let reopened = storage
            .generated_media_source_candidate(&access, resource)
            .unwrap()
            .unwrap();
        assert_eq!(reopened.source_fingerprint(), awaiting.source_fingerprint());
        assert_eq!(reopened.saved_path(), None);
        let prepared = storage
            .generated_media_source_candidate(&access, prepared_resource)
            .unwrap()
            .unwrap();
        assert_eq!(
            prepared.source_fingerprint(),
            completed.source_fingerprint()
        );
        assert_eq!(prepared.saved_path(), Some(Ok("C:\\durable.png")));
        assert!(
            storage
                .generated_media_preparation(prepared, asset, "image/png", None)
                .is_ok()
        );
        assert!(
            storage
                .unavailable_generated_media(reopened, Reason::LostSourceAuthority, timestamp(8))
                .is_ok()
        );
    }
    let store = publication.publish().unwrap();
    let reopened_generation = store.generation_identity().unwrap();
    assert_ne!(reopened_generation, original_generation);
    assert_eq!(store.health().generation().unwrap(), original_counter);
    assert!(matches!(
        storage.unavailable_generated_media(
            awaiting.clone(),
            Reason::LostSourceAuthority,
            timestamp(8)
        ),
        Err(SyndicMutationError::GeneratedMediaResourceCollision)
    ));
    assert!(matches!(
        storage.publish_generated_media(completed.clone(), asset, "image/png", None, timestamp(8)),
        Err(SyndicMutationError::GeneratedMediaResourceCollision)
    ));
    assert!(matches!(
        storage.generated_media_preparation(completed.clone(), asset, "image/png", None),
        Err(SyndicMutationError::GeneratedMediaResourceCollision)
    ));
    let mut stale = HomeCommand::new(store.home_revision().unwrap());
    stale
        .add_validation(state.assets().validate_generated_media_absence(
            state.assets().revision(&store).unwrap(),
            MediaHome::target(&awaiting),
            Reason::LostSourceAuthority,
        ))
        .unwrap();
    stale.add(stale_unavailable).unwrap();
    let outcome = store.execute(stale);
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "stale unavailable participant outcome: {outcome:?}"
    );
    let stale_admission = state
        .assets()
        .generated_media_admission(&store, MediaHome::target(&completed))
        .unwrap()
        .unwrap();
    let mut stale = HomeCommand::new(store.home_revision().unwrap());
    stale
        .add(
            state
                .assets()
                .publish_generated_media(
                    &store,
                    state.assets().revision(&store).unwrap(),
                    &stale_admission,
                )
                .unwrap(),
        )
        .unwrap();
    stale.add(stale_publication).unwrap();
    let outcome = store.execute(stale);
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "stale publication participant outcome: {outcome:?}"
    );
    let fresh = storage
        .generated_media_source(&store, prepared_resource)
        .unwrap()
        .unwrap();
    let target = MediaHome::target(&fresh);
    let admission = state
        .assets()
        .generated_media_admission(&store, target)
        .unwrap()
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            state
                .assets()
                .publish_generated_media(
                    &store,
                    state.assets().revision(&store).unwrap(),
                    &admission,
                )
                .unwrap(),
        )
        .unwrap();
    command
        .add(
            storage
                .publish_generated_media(fresh, asset, "image/png", None, timestamp(8))
                .unwrap(),
        )
        .unwrap();
    assert_committed(store.execute(command));
    assert_eq!(
        state
            .assets()
            .generated_media_admission(&store, target)
            .unwrap()
            .unwrap()
            .lifecycle(),
        GeneratedMediaLifecycle::Published
    );
    let awaiting = storage
        .generated_media_source(&store, resource)
        .unwrap()
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add_validation(state.assets().validate_generated_media_absence(
            state.assets().revision(&store).unwrap(),
            MediaHome::target(&awaiting),
            Reason::LostSourceAuthority,
        ))
        .unwrap();
    command
        .add(
            storage
                .unavailable_generated_media(awaiting, Reason::LostSourceAuthority, timestamp(8))
                .unwrap(),
        )
        .unwrap();
    assert_committed(store.execute(command));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn exact_source_replay_does_not_allocate_or_replace_generated_identity() {
    for incarnation in [
        None,
        Some(beryl_model::GeneratedSourceIncarnation::from_bytes(
            [0x47; 32],
        )),
    ] {
        let f = MediaHome::with_source_incarnation("generated-capture-replay", incarnation);
        let sealed = f.frame(
            20,
            false,
            ProviderImageGenerationStatusV1::InProgress,
            None,
            None,
            6,
        );
        let proof = f.proof(20);
        let source_event = source_events(&f.store, &f.storage, f.turn)
            .into_iter()
            .last()
            .unwrap();
        let state = f
            .storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap();
        let gate = f
            .storage
            .input_gate(&f.store, f.thread, limit())
            .unwrap()
            .unwrap();
        let replay = LiveSourceEvent::new(
            f.thread,
            f.turn,
            state.revision(),
            gate.revision(),
            source_event.sequence(),
            Some(f.source.clone()),
            SourceEventPayload::ItemFrame {
                item_id: proof.item_id(),
                frame: Box::new(sealed),
            },
            timestamp(6),
        )
        .unwrap();
        let activity = crate::support::fixture_activity(&f.store, &f.storage, f.thread, f.turn);
        let result = execute(
            &f.store,
            f.storage.admit_live_source_event(
                f.storage.revision(&f.store).unwrap(),
                replay,
                activity,
            ),
        );
        let CommandOutcome::NotCommitted { evidence } = result else {
            panic!("duplicate must refuse");
        };
        assert!(matches!(
            typed_error(&evidence),
            SyndicMutationError::SourceEventAlreadyAdmitted
        ));
        assert_eq!(
            f.storage
                .begin_generated_media_sweep(&f.store)
                .unwrap()
                .high_water(),
            1
        );
        assert_eq!(f.proof(20).source_fingerprint(), proof.source_fingerprint());
        assert_eq!(f.proof(20).source_incarnation(), incarnation);
    }
}

#[test]
fn publication_advances_only_the_next_frozen_item_and_preserves_completion_facts() {
    let f = MediaHome::new("generated-frozen-frontier");
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\first.png")),
        6,
    );
    f.frame(
        21,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\second.png")),
        7,
    );
    let captured = f.proof(20);
    admit_event(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.turn,
        &f.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(8),
    );
    complete_item_frontier(
        &f.store,
        &f.storage,
        f.thread,
        f.turn,
        TurnItemOrdinal::FIRST,
        SyndicItemId::from_bytes([4; 16]),
        timestamp(9),
    );
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    let item = f
        .storage
        .canonical_item(&f.store, captured.item_id(), limit())
        .unwrap()
        .unwrap();
    assert_committed(execute(
        &f.store,
        f.storage.freeze_next_turn_item(
            f.storage.revision(&f.store).unwrap(),
            FreezeNextTurnItem::new(
                f.thread,
                f.turn,
                state.revision(),
                item.ordinal(),
                item.id(),
                timestamp(10),
            ),
        ),
    ));
    let frozen = f.proof(20);
    assert_eq!(frozen.source_fingerprint(), captured.source_fingerprint());
    assert_ne!(frozen.item().revision(), captured.item().revision());
    assert_ne!(frozen.manifest().revision(), captured.manifest().revision());
    crate::support::exact_cas::project_item_if_needed(
        &f.store,
        f.storage.clone(),
        frozen.item_id(),
    );
    let asset = f.prepare(20);
    let events = source_events(&f.store, &f.storage, f.turn);
    let before = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    assert_committed(f.store.execute(f.publication(20, asset, 11)));
    let after = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(after.finalized_item_count(), 2);
    assert_eq!(after.item_count(), 3);
    assert_eq!(after.lifecycle(), before.lifecycle());
    assert_eq!(after.end_status(), before.end_status());
    assert_eq!(source_events(&f.store, &f.storage, f.turn), events);
    assert_eq!(
        f.proof(20).source_fingerprint(),
        captured.source_fingerprint()
    );
    assert_eq!(
        f.proof(21).resource().backing(),
        ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::PendingAsset)
    );
    assert!(
        !f.storage
            .history_summary(&f.store, f.thread, limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn unavailable_frozen_item_preserves_behind_frontier_and_terminal_gate_can_settle() {
    let f = MediaHome::new("generated-unavailable-frontier");
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Failed,
        None,
        None,
        6,
    );
    admit_event(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.turn,
        &f.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(7),
    );
    complete_item_frontier(
        &f.store,
        &f.storage,
        f.thread,
        f.turn,
        TurnItemOrdinal::FIRST,
        SyndicItemId::from_bytes([4; 16]),
        timestamp(8),
    );
    let item = f.proof(20).item().clone();
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    assert_committed(execute(
        &f.store,
        f.storage.freeze_next_turn_item(
            f.storage.revision(&f.store).unwrap(),
            FreezeNextTurnItem::new(
                f.thread,
                f.turn,
                state.revision(),
                item.ordinal(),
                item.id(),
                timestamp(9),
            ),
        ),
    ));
    assert_committed(f.unavailable(f.proof(20), Reason::ProducerFailed, 10));
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        1
    );
    converge_and_release_terminal_history(&f.store, f.storage.clone(), f.thread, f.turn);
    assert_eq!(
        f.storage
            .input_gate(&f.store, f.thread, limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    assert!(
        !f.storage
            .history_summary(&f.store, f.thread, limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        1
    );
}
