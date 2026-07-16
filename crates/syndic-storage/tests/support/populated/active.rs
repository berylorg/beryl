use std::num::NonZeroU64;

use beryl_model::{
    AcceptedInputRevision, AssetId, BindingRevision, CasItemId, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, DraftRevision,
    InputGateRevision, ProjectionRevision, SyndicDraftMarkerId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureRecord, fixture_advance_item_projection_digest, fixture_advance_transcript_digest,
    fixture_empty_projection, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use super::{
    active_item, active_projection, active_snapshot, build_item, cas_item, cas_thread, cas_turn,
    execution_binding, next_input, steering_input, suffix_item,
};
use crate::support::{
    composer_content_records, draft_id, empty_live_content_records,
    fixture_turn_state_with_capture, id, test_tool_profile, timestamp, utf8_content_records,
};

pub(super) fn records() -> Vec<FixtureRecord> {
    let thread = id(40);
    let draft = draft_id(41);
    let turn = super::active_turn();
    let item = active_item();
    let projection = active_projection();
    let projection_record = fixture_inline_paragraph_projection(item, turn, "active");
    let digest = root_turn_chain_digest(turn);
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_one = BindingRevision::new(1).unwrap();
    let binding_two = BindingRevision::new(2).unwrap();
    let binding_three = BindingRevision::new(3).unwrap();
    let selected = SelectedPathProof::new(Some(turn), thread_revision, digest);
    let cas_thread = cas_thread();
    let cas_turn = cas_turn();
    let cas_item = cas_item();
    let active_descriptor = SourceItemDescriptor::new(
        item,
        cas_item.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();
    let steering = steering_input();
    let next = next_input();
    let represented =
        CasRepresentedPrefixProof::new(None, thread_revision, empty_selected_path_digest());
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    let usable = UsableCasBinding::new(
        execution_binding(),
        cas_thread.clone(),
        represented,
        CasNativeTurnCount::ZERO,
        test_tool_profile(),
        lineage,
    );
    let steering_gate_revision = InputGateRevision::new(2).unwrap();
    let active_binding = ActiveCasBinding::new(
        usable.clone(),
        active_snapshot(),
        turn,
        steering_gate_revision,
        timestamp(8),
    );
    let pending_target =
        PendingSteeringTargetProof::new(binding_three, active_snapshot(), turn, cas_thread.clone());
    let steering_target = SteeringTargetProof::new(pending_target, cas_turn.clone());
    let gate_revision = InputGateRevision::new(3).unwrap();
    let (empty_content, empty_content_records) =
        composer_content_records(&ComposerPayload::default());
    let marker_id = SyndicDraftMarkerId::from_bytes([58; 16]);
    let marker_label = ImageLabelOrdinal::FIRST;
    let steering_payload =
        ComposerPayload::new(vec![ComposerAtom::image_marker(marker_id, marker_label)]).unwrap();
    let (steering_content, steering_content_records) = composer_content_records(&steering_payload);
    let (active_content, active_content_records) = utf8_content_records("active");
    let suffix_item = suffix_item();
    let build_item = build_item();
    let suffix_cas_item = CasItemId::new("active-suffix-item").unwrap();
    let build_cas_item = CasItemId::new("active-build-item").unwrap();
    let suffix_descriptor = SourceItemDescriptor::new(
        suffix_item,
        suffix_cas_item.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();
    let build_descriptor = SourceItemDescriptor::new(
        build_item,
        build_cas_item.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();
    let suffix_projection_record = fixture_empty_projection(suffix_item, turn);
    let suffix_projection = suffix_projection_record.id();
    let (suffix_content, suffix_content_records) = empty_live_content_records(suffix_item);
    let (build_content, build_content_records) = empty_live_content_records(build_item);
    let generation = ItemProjectionGeneration::FIRST;
    let digest_seed = fixture_item_projection_digest_seed();
    let active_projection_digest =
        fixture_advance_item_projection_digest(digest_seed, projection, projection_revision);
    let suffix_projection_digest =
        fixture_advance_item_projection_digest(digest_seed, suffix_projection, projection_revision);
    let transcript_entry = TranscriptViewEntryRecord::new(
        thread,
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        item,
        projection_revision,
        generation,
        projection,
        projection_revision,
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &transcript_entry);

    let mut records = vec![
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            thread_revision,
            Some(turn),
            draft,
            None,
            None,
            digest,
        )),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            ConversationParent::Turn(turn),
            None,
            None,
            empty_content,
            timestamp(6),
            timestamp(6),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                gate_revision,
                InputGateState::Steerable(steering_target.clone()),
                2,
                1,
                1,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(7),
        )),
        FixtureRecord::TurnState(fixture_turn_state_with_capture(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Active,
            5,
            3,
            1,
            2,
            0,
            timestamp(8),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemStarted {
                    item: active_descriptor.clone(),
                    assistant_phase: Some(AssistantMessagePhase::Commentary),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(2).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemDelta {
                    item_id: item,
                    cas_item_id: cas_item.clone(),
                    expected_kind: ProviderItemKind::AgentMessage,
                    text: SourceEventText::new("active").unwrap(),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(3).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemCompleted {
                    item: active_descriptor,
                    assistant_phase: Some(AssistantMessagePhase::Commentary),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(4).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemStarted {
                    item: suffix_descriptor.clone(),
                    assistant_phase: Some(AssistantMessagePhase::Commentary),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(5).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemStarted {
                    item: build_descriptor.clone(),
                    assistant_phase: Some(AssistantMessagePhase::Commentary),
                },
            )
            .unwrap(),
        ),
    ];
    records.extend(empty_content_records);
    records.extend(steering_content_records);
    records.extend(active_content_records);
    records.extend(suffix_content_records);
    records.extend(build_content_records);

    records.extend([
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_source_state(
                item,
                turn,
                TurnItemOrdinal::FIRST,
                projection_revision,
                Some(SourceEventSequence::new(3).unwrap()),
                3,
                Some(CasItemSource::new(
                    CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
                    cas_item.clone(),
                )),
                ProviderItemKind::AgentMessage,
                ProviderItemLifecycle::Completed,
                ProviderItemDisposition::CanonicalText,
                Some(AssistantMessagePhase::Commentary),
                CanonicalItemPayload::text(active_content),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            projection_revision,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::FIRST,
            turn,
            SourceEventSequence::FIRST,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::new(2).unwrap(),
            turn,
            SourceEventSequence::new(2).unwrap(),
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::new(3).unwrap(),
            turn,
            SourceEventSequence::new(3).unwrap(),
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_thread.clone(),
            cas_turn.clone(),
            cas_item,
            item,
            projection_revision,
        )),
        FixtureRecord::Projection(projection_record),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection,
            projection_revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            generation,
            ProjectionFormatVersion::V1,
            projection_revision,
            active_content,
            6,
            1,
            0,
            active_projection_digest,
            1,
            0,
            active_projection_digest,
            MarkdownParserCheckpoint::new(
                6,
                6,
                ContentPieceOrdinal::new(2).unwrap(),
                6,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            projection_revision,
            projection_revision,
            generation,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_source_state(
                suffix_item,
                turn,
                TurnItemOrdinal::new(2).unwrap(),
                projection_revision,
                Some(SourceEventSequence::new(4).unwrap()),
                1,
                Some(CasItemSource::new(
                    CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
                    suffix_cas_item.clone(),
                )),
                ProviderItemKind::AgentMessage,
                ProviderItemLifecycle::Started,
                ProviderItemDisposition::CanonicalText,
                Some(AssistantMessagePhase::Commentary),
                CanonicalItemPayload::text(suffix_content),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            suffix_item,
            projection_revision,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            suffix_item,
            ItemSourceEventOrdinal::FIRST,
            turn,
            SourceEventSequence::new(4).unwrap(),
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_thread.clone(),
            cas_turn.clone(),
            suffix_cas_item,
            suffix_item,
            projection_revision,
        )),
        FixtureRecord::Projection(suffix_projection_record),
        FixtureRecord::ItemProjection(ItemProjectionIndexRecord::new(
            suffix_item,
            generation,
            ProjectionOrdinal::FIRST,
            suffix_projection,
            projection_revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            suffix_item,
            generation,
            ProjectionFormatVersion::V1,
            projection_revision,
            suffix_content,
            0,
            0,
            0,
            digest_seed,
            1,
            0,
            suffix_projection_digest,
            MarkdownParserCheckpoint::new(
                0,
                0,
                ContentPieceOrdinal::FIRST,
                0,
                Box::<str>::default(),
                false,
                None,
            ),
            false,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            suffix_item,
            projection_revision,
            projection_revision,
            generation,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_source_state(
                build_item,
                turn,
                TurnItemOrdinal::new(3).unwrap(),
                projection_revision,
                Some(SourceEventSequence::new(5).unwrap()),
                1,
                Some(CasItemSource::new(
                    CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
                    build_cas_item.clone(),
                )),
                ProviderItemKind::AgentMessage,
                ProviderItemLifecycle::Started,
                ProviderItemDisposition::CanonicalText,
                Some(AssistantMessagePhase::Commentary),
                CanonicalItemPayload::text(build_content),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::new(3).unwrap(),
            build_item,
            projection_revision,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            build_item,
            ItemSourceEventOrdinal::FIRST,
            turn,
            SourceEventSequence::new(5).unwrap(),
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_thread.clone(),
            cas_turn.clone(),
            build_cas_item,
            build_item,
            projection_revision,
        )),
        FixtureRecord::ItemProjectionBuild(ItemProjectionBuildRecord::new(
            build_item,
            generation,
            projection_revision,
            ProjectionFormatVersion::V1,
            projection_revision,
            build_content,
            0,
            0,
            0,
            digest_seed,
            ItemProjectionBuildPhase::Parsing(MarkdownParserCheckpoint::new(
                0,
                0,
                ContentPieceOrdinal::FIRST,
                0,
                Box::<str>::default(),
                false,
                None,
            )),
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            1,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            thread_revision,
            Some(turn),
            digest,
            1,
            1,
            transcript_digest,
            false,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            turn,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Active,
            5,
            3,
            1,
            timestamp(8),
        )),
        FixtureRecord::TranscriptViewEntry(transcript_entry),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            thread_revision,
            Some(turn),
            digest,
            false,
            timestamp(8),
        )),
    ]);

    let accepted_revision = AcceptedInputRevision::new(1).unwrap();
    records.extend([
        FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
            steering,
            thread,
            accepted_revision,
            AcceptedInputOrdinal::FIRST,
            steering_gate_revision,
            AcceptedInputDisposition::SteerActiveTurn(steering_target),
            AcceptedInputLifecycle::Admitted,
            steering_content,
            1,
            timestamp(8),
        )),
        FixtureRecord::InputMarkerResolution(InputMarkerResolutionRecord::new(
            InputMarkerOwner::AcceptedInput(steering),
            InputMarkerOrdinal::FIRST,
            ResolvedImageMarker::new(
                marker_id,
                marker_label,
                AssetId::sha256_v1([59; 32], NonZeroU64::new(59).unwrap()),
            ),
        )),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread,
            AcceptedInputOrdinal::FIRST,
            steering,
            accepted_revision,
        )),
        FixtureRecord::AcceptedSteering(AcceptedSteeringIndexRecord::new(
            thread,
            turn,
            AcceptedInputOrdinal::FIRST,
            steering,
            accepted_revision,
        )),
        FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
            next,
            thread,
            accepted_revision,
            AcceptedInputOrdinal::new(2).unwrap(),
            gate_revision,
            AcceptedInputDisposition::NextTurn(NextTurnReason::WorkerCapacity),
            AcceptedInputLifecycle::Admitted,
            empty_content,
            0,
            timestamp(8),
        )),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread,
            AcceptedInputOrdinal::new(2).unwrap(),
            next,
            accepted_revision,
        )),
        FixtureRecord::AcceptedNextTurn(AcceptedNextTurnIndexRecord::new(
            thread,
            AcceptedInputOrdinal::new(2).unwrap(),
            next,
            accepted_revision,
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_one,
            selected,
            BindingState::unbound("active fixture history").unwrap(),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_two,
            selected,
            BindingState::valid(usable),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_three,
            selected,
            BindingState::active(active_binding),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_three,
            BindingLifecycle::Active,
            digest,
        )),
        FixtureRecord::ExecutionSnapshot(ExecutionSnapshotRecord::new(
            active_snapshot(),
            thread,
            binding_three,
            steering_gate_revision,
            turn,
            cas_thread.clone(),
            selected,
            represented,
            CasNativeTurnCount::ZERO,
            test_tool_profile(),
            lineage,
            execution_binding(),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            timestamp(8),
        )),
        FixtureRecord::ActiveCasTurn(ActiveCasTurnRecord::new(
            active_snapshot(),
            thread,
            turn,
            binding_three,
            cas_thread.clone(),
            cas_turn.clone(),
            timestamp(8),
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
            cas_thread.clone(),
            thread,
            binding_two,
            binding_three,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            cas_thread.clone(),
            thread,
            binding_two,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            cas_thread.clone(),
            thread,
            binding_three,
        )),
        FixtureRecord::CasTurn(CasTurnIndexRecord::new(
            cas_thread,
            cas_turn,
            thread,
            turn,
            binding_three,
            active_snapshot(),
            CasNativeTurnCount::new(1),
        )),
    ]);
    records
}
