use super::*;

#[test]
fn context_remains_valid_after_source_thread_selects_away_from_source_turn() {
    validate_seeded_and_reopen("context-source-moved", |store, storage| {
        let thread = id(30);
        let root = SyndicTurnId::from_bytes([29; 16]);
        let digest = root_turn_chain_digest(root);
        let limit = point_limit();
        let current_thread = required(
            storage.thread(store, thread, limit).unwrap(),
            "source thread",
        );
        let draft = required(
            storage
                .draft(store, current_thread.current_draft_id(), limit)
                .unwrap(),
            "source draft",
        );
        let current_head = required(
            storage.transcript_view_head(store, thread, limit).unwrap(),
            "source transcript head",
        );
        let current_binding = required(
            storage.current_binding(store, thread, limit).unwrap(),
            "source binding",
        );
        let gate = required(
            storage.input_gate(store, thread, limit).unwrap(),
            "source input gate",
        );
        let root_state = required(
            storage.turn_state(store, root, limit).unwrap(),
            "root state",
        );
        let thread_revision = current_thread.revision().checked_next().unwrap();
        let projection_revision = current_head.revision().checked_next().unwrap();
        let binding_revision = current_binding.binding().revision().checked_next().unwrap();
        let generation = current_head.generation().checked_next().unwrap();
        let selected = SelectedPathProof::new(Some(root), thread_revision, digest);
        let root_path = TranscriptPathTurnRecord::new(
            thread,
            generation,
            TurnDepth::FIRST,
            root,
            digest,
            root_state.revision(),
            root_state.lifecycle(),
            root_state.source_event_count(),
            root_state.item_count(),
            root_state.finalized_item_count(),
            root_state.updated_at(),
        );
        let mutation = batch([
            FixtureRecord::Thread(ThreadRecord::new(
                thread,
                selected,
                current_thread.current_draft_id(),
                current_thread.lineage(),
                current_thread.context_owner_id(),
            )),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                thread,
                draft.id(),
                draft.revision(),
                thread_revision,
            )),
            FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
                thread,
                generation,
                projection_revision,
                0,
                Some(root),
                digest,
                ProjectionLifecycle::Current,
            )),
            FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
                thread,
                generation,
                projection_revision,
                thread_revision,
                Some(root),
                digest,
                1,
                0,
                fixture_transcript_digest_seed(),
                true,
                TranscriptBuildPhase::Complete,
            )),
            FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                root_path.thread_id(),
                root_path.generation(),
                root_path.depth(),
                root_path.turn_id(),
                root_path.turn_path_digest(),
                root_path.state_revision(),
                root_path.lifecycle(),
                root_path.source_event_count(),
                root_path.item_count(),
                root_path.finalized_item_count(),
                root_path.updated_at(),
            )),
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                thread,
                current_head.revision().checked_next().unwrap(),
                thread_revision,
                Some(root),
                digest,
                true,
                root_state.updated_at().max(draft.updated_at()),
            )),
            FixtureRecord::Binding(BindingRecord::new(
                thread,
                binding_revision,
                selected,
                BindingState::unbound("source moved selection").unwrap(),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                thread,
                binding_revision,
                BindingLifecycle::Unbound,
                digest,
            )),
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    gate.thread_id(),
                    gate.revision().checked_next().unwrap(),
                    InputGateState::Idle,
                    gate.accepted_high_water(),
                    gate.route_generation_high_water(),
                    gate.selected_route(),
                    gate.live_steering_count(),
                    gate.live_next_turn_count(),
                    gate.live_logical_utf8_bytes(),
                )
                .unwrap(),
            ),
        ]);
        mutation
    });
}
