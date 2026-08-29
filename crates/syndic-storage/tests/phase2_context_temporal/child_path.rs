use super::*;

#[test]
fn submitted_context_owner_need_not_remain_on_child_selected_path() {
    validate_seeded_and_reopen("context-owner-off-later-path", |store, storage| {
        let source = source_turn();
        let child_thread = id(36);
        let old_draft = draft_id(37);
        let new_draft = draft_id(38);
        let owner_turn = old_draft.submitted_turn_id();
        let alternate = SyndicTurnId::from_bytes([39; 16]);
        let source_digest = child_turn_chain_digest(
            source,
            SyndicTurnId::from_bytes([29; 16]),
            root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
        );
        let owner_digest = child_turn_chain_digest(owner_turn, source, source_digest);
        let alternate_digest = child_turn_chain_digest(alternate, source, source_digest);
        let owner = DiscussionContextOwnerId::SubmittedTurn(owner_turn);
        let thread_revision = ThreadRevision::new(2).unwrap();
        let draft_revision = DraftRevision::new(1).unwrap();
        let head_revision = ProjectionRevision::new(2).unwrap();
        let binding_revision = BindingRevision::new(2).unwrap();
        let selected = SelectedPathProof::new(Some(alternate), thread_revision, alternate_digest);
        let generation = TranscriptGeneration::new(2).unwrap();
        let limit = point_limit();
        let source_item = required(
            storage.canonical_item(store, source_item(), limit).unwrap(),
            "source item",
        );
        let source_projection = required(
            storage
                .projection(store, source_projection(), limit)
                .unwrap(),
            "source projection",
        );
        let source_projection_head = required(
            storage
                .item_projection_head(store, source_item.id(), limit)
                .unwrap(),
            "source item projection head",
        );
        let source_state = required(
            storage.turn_state(store, source, limit).unwrap(),
            "source state",
        );
        let root = SyndicTurnId::from_bytes([29; 16]);
        let root_state = required(
            storage.turn_state(store, root, limit).unwrap(),
            "root state",
        );
        let transcript_entry = TranscriptViewEntryRecord::new(
            child_thread,
            generation,
            TranscriptPosition::FIRST,
            source_item.id(),
            source_item.revision(),
            source_projection_head.generation(),
            source_projection.id(),
            source_projection.revision(),
        );
        let transcript_digest =
            fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &transcript_entry);
        let new_root_history =
            seed_detached_canonical_draft_backing(store, storage, id(238), new_draft);

        let mut mutation = batch([
            FixtureRecord::Thread(ThreadRecord::new(
                child_thread,
                selected,
                new_draft,
                ThreadLineageProof::new(
                    Some(id(30)),
                    Some(id(30)),
                    syndic_storage::ThreadLineageDepth::new(2).unwrap(),
                    syndic_storage::child_thread_lineage_digest(
                        child_thread,
                        id(30),
                        syndic_storage::root_thread_lineage_digest(id(30)),
                    ),
                ),
                Some(owner),
            )),
            FixtureRecord::Draft(DraftRecord::new(
                new_draft,
                child_thread,
                draft_revision,
                DraftSubmissionIntent::Ordinary,
                new_root_history,
                timestamp(8),
                timestamp(8),
            )),
            FixtureRecord::ContextEnvelope(context_record(owner)),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                child_thread,
                new_draft,
                draft_revision,
                thread_revision,
            )),
            FixtureRecord::ThreadParent(ThreadParentIndexRecord::new(
                id(30),
                child_thread,
                thread_revision,
                owner,
            )),
            FixtureRecord::Turn(TurnRecord::new(
                owner_turn,
                child_thread,
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(source),
                Some(source),
                TurnDepth::new(3).unwrap(),
                owner_digest,
                timestamp(6),
            )),
            FixtureRecord::TurnState(fixture_turn_state(
                owner_turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                0,
                0,
                timestamp(6),
            )),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                source,
                owner_turn,
                TurnDepth::new(3).unwrap(),
                owner_digest,
            )),
            FixtureRecord::Turn(TurnRecord::new(
                alternate,
                child_thread,
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(source),
                Some(source),
                TurnDepth::new(3).unwrap(),
                alternate_digest,
                timestamp(7),
            )),
            FixtureRecord::TurnState(fixture_turn_state(
                alternate,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                1,
                0,
                timestamp(7),
            )),
            FixtureRecord::SourceEvent(
                SourceEventRecord::new(
                    alternate,
                    SourceEventSequence::FIRST,
                    None,
                    SourceEventPayload::TurnEnded(
                        TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                    ),
                )
                .unwrap(),
            ),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                source,
                alternate,
                TurnDepth::new(3).unwrap(),
                alternate_digest,
            )),
            FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
                child_thread,
                generation,
                head_revision,
                1,
                Some(alternate),
                alternate_digest,
                ProjectionLifecycle::Current,
            )),
            FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
                child_thread,
                generation,
                head_revision,
                thread_revision,
                Some(alternate),
                alternate_digest,
                3,
                1,
                transcript_digest,
                true,
                TranscriptBuildPhase::Complete,
            )),
            FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                child_thread,
                generation,
                TurnDepth::FIRST,
                root,
                root_turn_chain_digest(root),
                root_state.revision(),
                root_state.lifecycle(),
                root_state.source_event_count(),
                root_state.item_count(),
                root_state.finalized_item_count(),
                root_state.updated_at(),
            )),
            FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                child_thread,
                generation,
                TurnDepth::new(2).unwrap(),
                source,
                source_digest,
                source_state.revision(),
                source_state.lifecycle(),
                source_state.source_event_count(),
                source_state.item_count(),
                source_state.finalized_item_count(),
                source_state.updated_at(),
            )),
            FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                child_thread,
                generation,
                TurnDepth::new(3).unwrap(),
                alternate,
                alternate_digest,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                1,
                0,
                0,
                timestamp(7),
            )),
            FixtureRecord::TranscriptViewEntry(transcript_entry),
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                child_thread,
                head_revision,
                thread_revision,
                Some(alternate),
                alternate_digest,
                true,
                timestamp(8),
            )),
            FixtureRecord::Binding(BindingRecord::new(
                child_thread,
                binding_revision,
                selected,
                BindingState::unbound("submitted context owner retained").unwrap(),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                child_thread,
                binding_revision,
                BindingLifecycle::Unbound,
                alternate_digest,
            )),
        ]);
        mutation
            .delete(FixtureDelete::Draft(old_draft))
            .unwrap()
            .delete(FixtureDelete::ContextEnvelope(
                DiscussionContextOwnerId::Draft(old_draft),
            ))
            .unwrap();
        mutation
    });
}
