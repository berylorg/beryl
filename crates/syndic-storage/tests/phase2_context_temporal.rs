#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CursorReadLimits, DomainRegistrationError, DomainValidationError};
use beryl_model::{
    BindingRevision, DiscussionContextOwnerId, DraftRevision, InputGateRevision,
    ProjectionRevision, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use support::populated::{source_item, source_projection, source_resource_projection, source_turn};
use support::{
    TestHome, batch, commit, composer_content_records, draft_id, empty_composer_content,
    fixture_turn_state, fixture_turn_state_with_capture, id, open, seed_populated, timestamp,
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn required<T>(value: Option<T>, name: &str) -> T {
    value.unwrap_or_else(|| panic!("seeded {name} disappeared"))
}

fn snapshot_for(
    path: TranscriptPathTurnRecord,
    state: &TurnStateRecord,
) -> TranscriptPathTurnRecord {
    TranscriptPathTurnRecord::new(
        path.thread_id(),
        path.generation(),
        path.depth(),
        path.turn_id(),
        path.turn_path_digest(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.updated_at(),
    )
}

fn transcript_build_with_history_complete(
    build: TranscriptBuildRecord,
    history_complete: bool,
) -> TranscriptBuildRecord {
    TranscriptBuildRecord::new(
        build.thread_id(),
        build.generation(),
        build.revision(),
        build.source_thread_revision(),
        build.committed_tail(),
        build.selected_path_digest(),
        build.path_turn_count(),
        build.entry_count(),
        build.entry_digest(),
        history_complete,
        build.phase(),
    )
}

fn seeded_transcript_path(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    generation: TranscriptGeneration,
    turn: SyndicTurnId,
) -> TranscriptPathTurnRecord {
    storage
        .transcript_path_turns(
            store,
            thread,
            generation,
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .iter()
        .copied()
        .find(|path| path.turn_id() == turn)
        .unwrap_or_else(|| panic!("seeded transcript path has no requested turn"))
}

fn context_source_user_item_mutation() -> FixtureBatch {
    let thread = id(30);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let turn = SyndicTurnId::from_bytes([71; 16]);
    let item = beryl_model::SyndicItemId::from_bytes([70; 16]);
    let projection = fixture_inline_paragraph_projection(item, turn, "user");
    let revision = projection.revision();
    let (content, mut records) = composer_content_records(
        &ComposerPayload::new(vec![ComposerAtom::text("user").unwrap()]).unwrap(),
    );
    let item_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection.id(),
        revision,
    );
    let checkpoint = MarkdownParserCheckpoint::new(
        4,
        4,
        ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(2).unwrap()),
        4,
        Box::<str>::default(),
        false,
        None,
    );
    let context_source = DiscussionContextSource::new(
        thread,
        turn,
        item,
        projection.id(),
        revision,
        DiscussionContextRange::new(0, 4).unwrap(),
    );
    let context = DiscussionContextEnvelope::new(
        context_source,
        DiscussionContextText::new("user").unwrap(),
        timestamp(5),
    )
    .unwrap();
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(root),
            Some(root),
            TurnDepth::new(2).unwrap(),
            child_turn_chain_digest(turn, root, root_turn_chain_digest(root)),
            timestamp(5),
        )),
        FixtureRecord::TurnState(fixture_turn_state_with_capture(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            0,
            1,
            1,
            1,
            0,
            timestamp(5),
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            root,
            turn,
            TurnDepth::new(2).unwrap(),
            child_turn_chain_digest(turn, root, root_turn_chain_digest(root)),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
        FixtureRecord::Projection(projection.clone()),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection.id(),
            revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            revision,
            ProjectionTextSource::composer(content),
            4,
            1,
            0,
            item_digest,
            1,
            0,
            item_digest,
            checkpoint,
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::ContextEnvelope(ContextEnvelopeRecord::new(
            DiscussionContextOwnerId::Draft(draft_id(37)),
            ContextEnvelopeRevision::FIRST,
            context,
        )),
    ]);
    batch(records)
}

fn context_record(owner: DiscussionContextOwnerId) -> ContextEnvelopeRecord {
    context_record_with_text(owner, "assistant")
}

fn context_record_with_text(owner: DiscussionContextOwnerId, text: &str) -> ContextEnvelopeRecord {
    context_record_with_projection(owner, source_projection(), text)
}

fn context_record_with_projection(
    owner: DiscussionContextOwnerId,
    projection: beryl_model::SyndicProjectionId,
    text: &str,
) -> ContextEnvelopeRecord {
    context_record_with_projection_revision(
        owner,
        projection,
        ProjectionRevision::new(1).unwrap(),
        text,
    )
}

fn context_record_with_projection_revision(
    owner: DiscussionContextOwnerId,
    projection: beryl_model::SyndicProjectionId,
    revision: ProjectionRevision,
    text: &str,
) -> ContextEnvelopeRecord {
    let source = DiscussionContextSource::new(
        id(30),
        source_turn(),
        source_item(),
        projection,
        revision,
        DiscussionContextRange::new(0, 9).unwrap(),
    );
    let envelope = DiscussionContextEnvelope::new(
        source,
        DiscussionContextText::new(text).unwrap(),
        timestamp(5),
    )
    .unwrap();
    ContextEnvelopeRecord::new(owner, ContextEnvelopeRevision::FIRST, envelope)
}

fn assert_context_rejection(name: &str, expected: &str, mutation: FixtureBatch) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, mutation);
    match store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err()
        .validation_error()
    {
        DomainValidationError::Rejected { domain, source } => {
            assert_eq!(*domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected context semantic rejection, got {other:?}"),
    }
    let candidate = store.recover_same_home().unwrap();
    SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    SyndicStorage::reacquire(&recovered).unwrap();
    recovered.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("corrupt context reopened successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected context registration rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}

fn assert_seeded_context_rejection(
    name: &str,
    expected: &str,
    mutation: impl Fn(&beryl_home_store::HomeStore, SyndicStorage) -> FixtureBatch,
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, mutation(&store, storage));
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("corrupt context reopened successfully"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(expected), "{error}");
    reopened.close().unwrap();
}

fn validate_seeded_and_reopen(
    name: &str,
    mutation: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) -> FixtureBatch,
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    commit(&store, storage, mutation(&store, storage));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

fn unknown_terminal_source_mutation(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
) -> FixtureBatch {
    let point = SyndicPointReadLimit::new(1_000_000).unwrap();
    let events = storage
        .source_events(
            store,
            source_turn(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let terminal = events
        .records()
        .iter()
        .find(|event| matches!(event.payload(), SourceEventPayload::TurnEnded(_)))
        .unwrap_or_else(|| panic!("seeded source turn has no terminal event"));
    let item = required(
        storage.canonical_item(store, source_item(), point).unwrap(),
        "source canonical item",
    );
    let (item_event, frame) = events
        .records()
        .iter()
        .find_map(|event| match event.payload() {
            SourceEventPayload::ItemFrame { item_id, frame } if *item_id == source_item() => {
                (Some(event.sequence()) == item.source_event())
                    .then(|| (event.sequence(), frame.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("seeded source item frame disappeared"));
    let item_head = required(
        storage
            .item_projection_head(store, item.id(), point)
            .unwrap(),
        "source item projection head",
    );
    let item_set = required(
        storage
            .item_projection_set(store, item.id(), item_head.generation(), point)
            .unwrap(),
        "source item projection set",
    );
    let prior_generation = ItemProjectionGeneration::new(item_head.generation().get() - 1).unwrap();
    let prior_set = required(
        storage
            .item_projection_set(store, item.id(), prior_generation, point)
            .unwrap(),
        "pre-finalization source item projection set",
    );
    let retired_projections = storage
        .item_projections(
            store,
            item.id(),
            item_head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec();
    let live_revision = ProjectionRevision::new(item.revision().get() - 1).unwrap();
    let (provider, manifest) =
        syndic_storage::test_faults::fixture_provider_content_manifest(item.id(), &frame, false);
    let cas_source = item.cas_source().unwrap().clone();
    let live_item = CanonicalItemRecord::with_provider_state(
        item.id(),
        item.turn_id(),
        item.ordinal(),
        live_revision,
        item_event,
        3,
        cas_source.clone(),
        item.assistant_phase(),
        provider,
        item.narrative_completion(),
        item.presentation().clone(),
    )
    .unwrap();
    let state = required(
        storage.turn_state(store, source_turn(), point).unwrap(),
        "source state",
    );
    let gate = required(
        storage.input_gate(store, id(30), point).unwrap(),
        "source input gate",
    );
    let transcript_head = required(
        storage.transcript_view_head(store, id(30), point).unwrap(),
        "source transcript head",
    );
    let prior_transcript_generation =
        TranscriptGeneration::new(transcript_head.generation().get() - 1).unwrap();
    let retired_transcript_entries = storage
        .transcript_entries(
            store,
            id(30),
            transcript_head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec();
    let retired_transcript_path = storage
        .transcript_path_turns(
            store,
            id(30),
            transcript_head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec();
    let prior_transcript = required(
        storage
            .transcript_build(store, id(30), prior_transcript_generation, point)
            .unwrap(),
        "pre-finalization source transcript build",
    );
    let prior_path = seeded_transcript_path(
        store,
        storage,
        id(30),
        prior_transcript_generation,
        source_turn(),
    );
    let activity_head = required(
        storage.activity_query_head(store, id(30), point).unwrap(),
        "source activity head",
    );
    let activity_source = storage
        .activity_query_source_page(
            store,
            &activity_head,
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .iter()
        .find(|member| member.source().turn_id() == source_turn())
        .cloned()
        .unwrap_or_else(|| panic!("seeded activity source disappeared"));
    let summary = required(
        storage.history_summary(store, id(30), point).unwrap(),
        "source history summary",
    );
    let status = TurnEndStatus::new(TurnTerminalOutcome::UnknownTerminal, None).unwrap();
    let unknown = TurnStateRecord::with_capture_frontiers_and_issue(
        state.turn_id(),
        state.revision(),
        TurnLifecycle::UnknownTerminal,
        state.source_event_count(),
        state.item_count(),
        0,
        state.open_item_count(),
        state.history_blocking_item_count(),
        state.provider_observation_issue(),
        Some(status),
        state.updated_at(),
    )
    .unwrap();
    let mut mutation = batch([
        FixtureRecord::InputGate(
            InputGateRecord::new(
                gate.thread_id(),
                gate.revision(),
                InputGateState::PendingTurn(source_turn()),
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                gate.selected_route(),
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnState(unknown.clone()),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source_turn(),
                terminal.sequence(),
                terminal.source().cloned(),
                SourceEventPayload::TurnEnded(status),
            )
            .unwrap(),
        ),
        FixtureRecord::ContentManifest(manifest),
        FixtureRecord::CanonicalItem(live_item),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            item.turn_id(),
            item.ordinal(),
            item.id(),
            live_revision,
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_source.turn().thread_id().clone(),
            cas_source.turn().turn_id().clone(),
            cas_source.item_id().clone(),
            item.id(),
            live_revision,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item_head.item_id(),
            item_head.revision(),
            live_revision,
            prior_set.generation(),
            item_head.lifecycle(),
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            transcript_head.thread_id(),
            prior_transcript.generation(),
            prior_transcript.revision(),
            prior_transcript.entry_count(),
            prior_transcript.committed_tail(),
            prior_transcript.selected_path_digest(),
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptPathTurn(snapshot_for(prior_path, &unknown)),
        FixtureRecord::TranscriptBuild(transcript_build_with_history_complete(
            prior_transcript,
            false,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            summary.thread_id(),
            summary.revision(),
            summary.thread_revision(),
            summary.committed_tail(),
            summary.selected_path_digest(),
            false,
            summary.last_activity_at(),
        )),
        FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                activity_head.thread_id(),
                activity_head.work_period(),
                activity_head.source(),
                true,
                activity_head.source_frontier(),
                activity_head.revision(),
                activity_head.source_count(),
                activity_head.logical_row_count(),
                activity_head.running_row_count(),
                activity_head.completed_row_count(),
                activity_head.completed_stored_bytes(),
                activity_head.completed_retention_cutoff(),
                activity_head.lifecycle(),
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
            activity_source.thread_id(),
            activity_source.work_period(),
            activity_source.source(),
            activity_source.activity_start(),
            activity_source.source_frontier(),
            true,
            activity_source.child_handoff(),
        )),
    ]);
    mutation
        .delete(FixtureDelete::ItemProjectionSet {
            item: item.id(),
            generation: item_set.generation(),
        })
        .unwrap();
    for projection in retired_projections {
        mutation
            .delete(FixtureDelete::ItemProjection {
                item: projection.item_id(),
                generation: projection.generation(),
                ordinal: projection.ordinal(),
            })
            .unwrap();
    }
    mutation
        .delete(FixtureDelete::TranscriptBuild {
            thread: transcript_head.thread_id(),
            generation: transcript_head.generation(),
        })
        .unwrap();
    for entry in retired_transcript_entries {
        mutation
            .delete(FixtureDelete::TranscriptViewEntry {
                thread: entry.thread_id(),
                generation: entry.generation(),
                position: entry.position(),
            })
            .unwrap();
    }
    for path in retired_transcript_path {
        mutation
            .delete(FixtureDelete::TranscriptPathTurn {
                thread: path.thread_id(),
                generation: path.generation(),
                depth: path.depth(),
            })
            .unwrap();
    }
    mutation
}

#[path = "phase2_context_temporal/child_path.rs"]
mod child_path;
#[path = "phase2_context_temporal/reopen_validation.rs"]
mod reopen_validation;
#[path = "phase2_context_temporal/selected_away.rs"]
mod selected_away;
