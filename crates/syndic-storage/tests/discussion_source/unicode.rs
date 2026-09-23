use super::*;
use beryl_model::{CasItemId, SyndicItemId};
use support::exact_cas::{
    admit_event, admit_item_frame, converge_and_release_terminal_history, correlate_user_item,
    establish_turn, submit_current_draft,
};
use support::{draft_id, timestamp};

#[test]
fn multibyte_selection_obeys_projection_and_codepoint_boundaries() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let storage = &fixture.storage;
    let thread = id(201);
    support::seed_canonical_empty_thread(store, storage.clone(), thread, draft_id(202));
    let turn = submit_current_draft(
        store,
        storage.clone(),
        thread,
        draft_id(203),
        SyndicItemId::from_bytes([204; 16]),
        "question",
        timestamp(2),
    );
    let route = establish_turn(store, storage.clone(), thread, turn, timestamp(3));
    admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        &route,
        SourceEventPayload::TurnActivated,
        timestamp(3),
    );
    correlate_user_item(
        store,
        storage.clone(),
        thread,
        turn,
        SyndicItemId::from_bytes([204; 16]),
        &route,
        timestamp(3),
    );
    let item = SyndicItemId::from_bytes([205; 16]);
    let text = "🦀".repeat(DISCUSSION_CONTEXT_MAX_BYTES / 4);
    let agent = |text: &str| {
        ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
            text: ProviderTextV1::inline(text),
            phase: Some(ProviderMessagePhaseV1::FinalAnswer),
            memory_citation: None,
        })
    };
    let frames = [
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(4),
            item: agent(""),
        },
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
            delta: ProviderTextV1::inline(&text),
        }),
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(5),
            item: agent(&text),
        },
    ];
    for (index, observation) in frames.into_iter().enumerate() {
        admit_item_frame(
            store,
            storage.clone(),
            thread,
            turn,
            item,
            &route,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::new(index as u64 + 1).unwrap(),
                CasItemId::new("unicode-answer").unwrap(),
                observation,
            ),
            timestamp(5),
        );
    }
    admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        &route,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(6),
    );
    converge_and_release_terminal_history(store, storage.clone(), thread, turn);
    let thread_record = storage.thread(store, thread, limit()).unwrap().unwrap();
    let head = storage
        .transcript_view_head(store, thread, limit())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            thread,
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == item)
        .unwrap();
    let source = |start, end| {
        DiscussionContextSource::new(
            thread,
            turn,
            item,
            entry.projection_id(),
            entry.projection_revision(),
            DiscussionContextRange::new(start, end).unwrap(),
        )
    };
    let entry_proof = CurrentTranscriptEntryProof::new(head.generation(), entry.position());
    let prepare = |start, end, selected: &str| {
        storage.prepare_discussion_source(
            store,
            source(start, end),
            thread_record.selected_path(),
            entry_proof,
            DiscussionContextText::new(selected).unwrap(),
        )
    };
    let projection = storage
        .projection(store, entry.projection_id(), limit())
        .unwrap()
        .unwrap();
    let range = projection.payload().source_range().unwrap();
    assert_eq!(range.start(), 0);
    assert!(range.end() < text.len() as u64);
    let selected = &text[..range.end() as usize];
    let proof = prepare(0, range.end(), selected).unwrap();
    assert_eq!(proof.text().as_str(), selected);
    assert!(prepare(0, text.len() as u64, &text).is_err());
    assert!(prepare(0, 4, "🦀").is_ok());
    assert!(prepare(1, 5, "🦀").is_err());
    assert!(prepare(0, 3, "abc").is_err());
    assert!(prepare(1, 4, "abc").is_err());
    assert!(matches!(
        fixture.validate(proof),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}
