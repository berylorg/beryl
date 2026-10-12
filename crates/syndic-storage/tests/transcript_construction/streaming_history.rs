use super::*;
use syndic_storage::{
    ProviderAgentMessageV1, ProviderItemDeltaV1, ProviderItemLifecycle, TurnLifecycle,
};

fn publish_snapshot(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> Vec<syndic_storage::TranscriptViewEntryRecord> {
    let (generation, _) = start_transcript_build(store, storage.clone(), thread);
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return transcript_entries(store, storage, thread, generation);
        }
        advance_transcript_build(store, storage.clone(), thread, generation);
    }
    panic!("streaming snapshot exceeded its construction bound");
}

fn agent_value(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: None,
        memory_citation: None,
    })
}

#[test]
fn saved_streaming_code_publishes_before_completion_and_retains_prior_snapshots() {
    let home = TestHome::new("transcript-streaming-code");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let store = store
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = create_thread(&store, storage.clone());
    let submitted = submit_text(
        &store,
        storage.clone(),
        thread,
        "show code",
        draft_id(3),
        SyndicItemId::from_bytes([40; 16]),
        timestamp(3),
    );
    let source = establish_turn(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        timestamp(4),
    );
    admit(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_user_item(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        submitted.item,
        &source,
        timestamp(5),
    );
    project_item(&store, storage.clone(), submitted.item);
    let assistant = SyndicItemId::from_bytes([41; 16]);
    let cas_item = CasItemId::new("streaming-code").unwrap();
    let initial = "```rust\n";
    admit_item_frame(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(6),
                item: agent_value(&initial),
            },
        ),
        timestamp(6),
    );
    project_item(&store, storage.clone(), assistant);
    let first = publish_snapshot(&store, storage.clone(), thread);
    assert_eq!(first.len(), 2);
    assert_eq!(first[1].item_id(), assistant);
    let resources = projection_resource_ids(&store, storage.clone(), &[first[1].projection_id()]);
    assert_eq!(resources.len(), 1);
    let state = storage
        .turn_state(&store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Active);
    assert_eq!(state.finalized_item_count(), 0);
    assert_eq!(
        storage
            .canonical_item(&store, assistant, point_limit())
            .unwrap()
            .unwrap()
            .provider_lifecycle(),
        ProviderItemLifecycle::Started
    );
    assert!(
        !storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );

    store.close().unwrap();
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let store = store
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        transcript_entries(&store, storage.clone(), thread, first[0].generation()),
        first
    );

    let delta = "let answer = 42;\n```\n";
    admit_item_frame(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline(delta),
            }),
        ),
        timestamp(7),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    project_item(&store, storage.clone(), assistant);
    let second = publish_snapshot(&store, storage.clone(), thread);
    assert_eq!(second.len(), 2);
    assert_eq!(second[1].item_id(), assistant);
    assert!(second[1].item_revision() > first[1].item_revision());
    assert_eq!(
        transcript_entries(&store, storage.clone(), thread, first[0].generation()),
        first
    );
    assert_eq!(
        storage
            .turn_state(&store, submitted.turn, point_limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        0
    );

    let completed_text = format!("{initial}{delta}");
    admit_item_frame(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(3).unwrap(),
            cas_item,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(8),
                item: agent_value(&completed_text),
            },
        ),
        timestamp(8),
    );
    admit(
        &store,
        storage.clone(),
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(9),
    );
    converge_and_release_terminal_history(&store, storage.clone(), thread, submitted.turn);
    assert!(
        storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let reopened = reopened
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        transcript_entries(&reopened, storage.clone(), thread, first[0].generation()),
        first
    );
    assert_eq!(
        transcript_entries(&reopened, storage, thread, second[0].generation()),
        second
    );
    reopened.close().unwrap();
}
