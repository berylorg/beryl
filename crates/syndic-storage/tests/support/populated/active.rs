mod item_records;
mod provider_seed;
mod route_records;

use beryl_model::{
    AcceptedInputRevision, AssetReferenceSetDigest, AssetReferenceSetId, BindingRevision,
    CasItemId, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasProcessGeneration, DraftRevision, InputGateRevision, ProjectionRevision,
    SealedAssetReferenceSetProof, SyndicDraftMarkerId, ThreadRevision,
};
use syndic_storage::test_faults::{
    fixture_activity_query_entry_stored_bytes, fixture_advance_item_projection_digest,
    fixture_empty_projection, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_route_leaf_with_transition,
    fixture_transcript_digest_seed, FixtureRecord,
};
use syndic_storage::*;

use super::provider::ProviderSeedTurn;
use super::{
    active_item, active_projection, active_snapshot, activity_item, agent_item_fixture, build_item,
    cas_item, cas_thread, cas_turn, command_item_fixture, execution_binding, next_input,
    steering_input, suffix_item, AgentItemFixtureState,
};
use crate::support::{
    canonical_empty_root_history_pair_for, composer_content_records, draft_id,
    fixture_turn_state_with_capture, id, test_tool_profile, timestamp,
};

pub(super) use provider_seed::seed_provider_records;

pub(super) fn records() -> Vec<FixtureRecord> {
    let thread = id(40);
    let draft = draft_id(41);
    let turn = super::active_turn();
    let item = active_item();
    let projection = active_projection();
    let projection_record = fixture_inline_paragraph_projection(item, turn, "active");
    let digest = root_turn_chain_digest(turn);
    let thread_revision = ThreadRevision::new(3).unwrap();
    let binding_thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let active_item_revision = ProjectionRevision::new(3).unwrap();
    let binding_one = BindingRevision::new(1).unwrap();
    let binding_two = BindingRevision::new(2).unwrap();
    let binding_three = BindingRevision::new(3).unwrap();
    let selected = SelectedPathProof::new(Some(turn), binding_thread_revision, digest);
    let cas_thread = cas_thread();
    let cas_turn = cas_turn();
    let cas_item = cas_item();
    let active_source = CasItemSource::new(
        CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
        cas_item.clone(),
    );
    let active_provider = agent_item_fixture(
        item,
        turn,
        active_source.clone(),
        SourceEventSequence::FIRST,
        ProviderMessagePhaseV1::Commentary,
        "active",
        AgentItemFixtureState::Completed,
    );
    let active_text_source =
        ProjectionTextSource::provider_narrative(active_provider.canonical.narrative().unwrap());
    let steering = steering_input();
    let next = next_input();
    let represented =
        CasRepresentedPrefixProof::new(None, binding_thread_revision, empty_selected_path_digest());
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
    let gate_revision = InputGateRevision::new(4).unwrap();
    let (empty_content, empty_content_records) =
        composer_content_records(&ComposerPayload::default());
    let marker_id = SyndicDraftMarkerId::from_bytes([58; 16]);
    let marker_label = ImageLabelOrdinal::FIRST;
    let steering_payload =
        ComposerPayload::new(vec![ComposerAtom::image_marker(marker_id, marker_label)]).unwrap();
    let (steering_content, steering_content_records) = composer_content_records(&steering_payload);
    let steering_source = steering_content.sealed_marker_summary().unwrap();
    let steering_asset_reference_set = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([59; 16]),
        steering_source,
        steering_source.marker_count(),
        AssetReferenceSetDigest::from_bytes([59; 32]),
    )
    .unwrap();
    let suffix_item = suffix_item();
    let build_item = build_item();
    let activity_item = activity_item();
    let suffix_cas_item = CasItemId::new("active-suffix-item").unwrap();
    let build_cas_item = CasItemId::new("active-build-item").unwrap();
    let activity_cas_item = CasItemId::new("active-activity-item").unwrap();
    let suffix_source = CasItemSource::new(
        CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
        suffix_cas_item.clone(),
    );
    let suffix_provider = agent_item_fixture(
        suffix_item,
        turn,
        suffix_source.clone(),
        SourceEventSequence::new(4).unwrap(),
        ProviderMessagePhaseV1::Commentary,
        "",
        AgentItemFixtureState::Live,
    );
    let suffix_text_source =
        ProjectionTextSource::provider_narrative(suffix_provider.canonical.narrative().unwrap());
    let build_source = CasItemSource::new(
        CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
        build_cas_item.clone(),
    );
    let build_provider = agent_item_fixture(
        build_item,
        turn,
        build_source.clone(),
        SourceEventSequence::new(5).unwrap(),
        ProviderMessagePhaseV1::Commentary,
        "",
        AgentItemFixtureState::Live,
    );
    let activity_source = CasItemSource::new(
        CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
        activity_cas_item.clone(),
    );
    let activity_provider = command_item_fixture(
        activity_item,
        turn,
        activity_source.clone(),
        SourceEventSequence::new(6).unwrap(),
    );
    let activity_order = ActivityQueryOrder::new(false, timestamp(1), activity_item);
    let activity_entry = ActivityQueryEntryRecord::new(
        thread,
        ActivityWorkPeriod::FIRST,
        activity_order,
        ActivityItemSource::new(thread, turn, activity_item, activity_source.clone()),
        SourceEventSequence::new(7).unwrap(),
        ProviderItemKind::CommandExecution,
        ProviderItemLifecycle::Completed,
        None,
    )
    .unwrap();
    let activity_stored_bytes = fixture_activity_query_entry_stored_bytes(&activity_entry);
    let build_text_source =
        ProjectionTextSource::provider_narrative(build_provider.canonical.narrative().unwrap());
    let suffix_projection_record = fixture_empty_projection(suffix_item, turn);
    let suffix_projection = suffix_projection_record.id();
    let generation = ItemProjectionGeneration::FIRST;
    let digest_seed = fixture_item_projection_digest_seed();
    let active_projection_digest =
        fixture_advance_item_projection_digest(digest_seed, projection, projection_revision);
    let suffix_projection_digest =
        fixture_advance_item_projection_digest(digest_seed, suffix_projection, projection_revision);
    let transcript_digest = fixture_transcript_digest_seed();
    let thread_execution = ThreadExecutionRecord::new(thread, execution_binding());
    let thread_attributes = ThreadAttributesRecord::ordinary(thread);
    let thread_usage = ThreadUsageRecord::empty(thread);
    let thread_catalog = ThreadCatalogSummaryRecord::new(
        thread,
        ProjectionRevision::new(1).unwrap(),
        None,
        execution_binding(),
        ThreadArchiveState::Ordinary,
        timestamp(8),
        false,
        None,
        ThreadLineageDepth::FIRST,
        root_thread_lineage_digest(thread),
        ThreadCatalogSourceWitnesses::new(
            thread_attributes.revision(),
            projection_revision,
            thread_revision,
            digest,
            thread_revision,
        ),
    );

    let mut records = vec![
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            SelectedPathProof::new(Some(turn), thread_revision, digest),
            draft,
            ThreadLineageProof::new(
                None,
                None,
                syndic_storage::ThreadLineageDepth::FIRST,
                syndic_storage::root_thread_lineage_digest(thread),
            ),
            syndic_storage::ThreadImageLabelFrontiers::new(
                syndic_storage::ImageLabelFrontier::EMPTY,
                syndic_storage::ImageLabelFrontier::from_raw(1),
            )
            .unwrap(),
            None,
        )),
        FixtureRecord::ThreadExecution(thread_execution),
        FixtureRecord::ThreadAttributes(thread_attributes),
        FixtureRecord::ThreadUsage(thread_usage),
        FixtureRecord::ThreadCatalogSummary(thread_catalog),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
            canonical_empty_root_history_pair_for(draft),
            timestamp(8),
            timestamp(8),
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
                InputGateState::Steerable(turn),
                2,
                Some(AcceptedRouteGeneration::FIRST),
                Some(AcceptedRouteHeadProof::new(
                    AcceptedRouteGeneration::FIRST,
                    AcceptedRouteRevision::new(2).unwrap(),
                )),
                1,
                1,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                thread,
                ActivityWorkPeriod::FIRST,
                Some(ActivityQuerySource::new(thread, turn)),
                true,
                7,
                ActivityQueryRevision::FIRST,
                1,
                1,
                0,
                1,
                activity_stored_bytes,
                Some(activity_order),
                ProjectionLifecycle::Current,
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
            thread,
            ActivityWorkPeriod::FIRST,
            ActivityQuerySource::new(thread, turn),
            Some(SourceEventSequence::new(6).unwrap()),
            7,
            true,
            None,
        )),
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
            7,
            4,
            0,
            2,
            0,
            timestamp(8),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(active_provider.frames[0].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(2).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(active_provider.frames[1].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(3).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(active_provider.frames[2].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(4).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: suffix_item,
                    frame: Box::new(suffix_provider.frames[0].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(5).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: build_item,
                    frame: Box::new(build_provider.frames[0].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(6).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: activity_item,
                    frame: Box::new(activity_provider.frames[0].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::new(7).unwrap(),
                Some(CasTurnSource::new(cas_thread.clone(), cas_turn.clone())),
                SourceEventPayload::ItemFrame {
                    item_id: activity_item,
                    frame: Box::new(activity_provider.frames[1].clone()),
                },
            )
            .unwrap(),
        ),
    ];
    records.extend(empty_content_records);
    records.extend(steering_content_records);

    records.extend(item_records::records(item_records::ItemRecordFacts {
        item,
        turn,
        active_item_revision,
        active_source,
        active_canonical: active_provider.canonical,
        cas_thread: cas_thread.clone(),
        cas_turn: cas_turn.clone(),
        cas_item,
        projection_record,
        projection,
        projection_revision,
        generation,
        active_text_source,
        active_projection_digest,
        suffix_item,
        suffix_source,
        suffix_canonical: suffix_provider.canonical,
        suffix_cas_item,
        suffix_projection_record,
        suffix_projection,
        suffix_text_source,
        digest_seed,
        suffix_projection_digest,
        build_item,
        build_source,
        build_canonical: build_provider.canonical,
        build_cas_item,
        build_text_source,
        activity_item,
        activity_source,
        activity_canonical: activity_provider.canonical,
        activity_cas_item,
        activity_entry,
        thread,
        digest,
        thread_revision,
        transcript_digest,
    }));

    records.extend(route_records::records(route_records::RouteRecordFacts {
        thread,
        current_draft: draft,
        steering,
        steering_gate_revision,
        steering_content,
        steering_asset_reference_set,
        marker_label,
        steering_target,
        next,
        gate_revision,
        empty_content,
        binding_one,
        binding_two,
        binding_three,
        selected,
        usable,
        active_binding,
        digest,
        turn,
        cas_thread,
        represented,
        lineage,
        cas_turn,
    }));

    records
}
