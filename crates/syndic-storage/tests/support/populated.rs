mod active;
mod context;

use beryl_model::{
    BindingRevision, CasItemId, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId, DraftRevision,
    InputGateRevision, ProjectionRevision, SyndicAcceptedInputId, SyndicExecutionSnapshotId,
    SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureRecord, fixture_advance_item_projection_digest, fixture_advance_transcript_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
    fixture_transcript_digest_seed,
};
use syndic_storage::*;

use super::{
    composer_content_records, draft_id, fixture_turn_state, id, test_tool_profile, timestamp,
    utf8_content_records,
};

pub fn source_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([32; 16])
}

pub fn source_item() -> SyndicItemId {
    SyndicItemId::from_bytes([33; 16])
}

fn source_cas_thread() -> CasThreadId {
    CasThreadId::new("source-history-thread").unwrap()
}

fn source_cas_turn() -> CasTurnId {
    CasTurnId::new("source-history-turn").unwrap()
}

fn source_cas_item() -> CasItemId {
    CasItemId::new("source-history-item").unwrap()
}

pub fn source_cas_authority() -> CasTurnSource {
    CasTurnSource::new(source_cas_thread(), source_cas_turn())
}

fn source_snapshot() -> SyndicExecutionSnapshotId {
    SyndicExecutionSnapshotId::from_bytes([35; 16])
}

pub fn source_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_inline_paragraph_projection(
        source_item(),
        source_turn(),
        "assistant",
    )
    .id()
}

pub fn correlate_source_user_item(
    records: &mut Vec<FixtureRecord>,
    item: SyndicItemId,
    revision: ProjectionRevision,
    content: ContentReference,
    marker_count: u64,
    updated_at: SyndicTimestamp,
) {
    let source = source_turn();
    let source_thread = id(30);
    let source_digest = child_turn_chain_digest(
        source,
        SyndicTurnId::from_bytes([29; 16]),
        root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
    );
    records.retain(|record| {
        !matches!(record, FixtureRecord::TurnState(state) if state.turn_id() == source)
            && !matches!(record, FixtureRecord::SourceEvent(event)
                if event.turn_id() == source && event.sequence().get() >= 5)
            && !matches!(record, FixtureRecord::CanonicalItem(existing) if existing.id() == item)
            && !matches!(record, FixtureRecord::TurnItem(index)
                if index.turn_id() == source && index.ordinal() == TurnItemOrdinal::new(2).unwrap())
            && !matches!(record, FixtureRecord::ItemSourceEvent(index) if index.item_id() == item)
            && !matches!(record, FixtureRecord::CasItem(index) if index.item_id() == item)
            && !matches!(record, FixtureRecord::TranscriptPathTurn(path)
                if path.thread_id() == source_thread
                    && path.generation() == TranscriptGeneration::FIRST
                    && path.depth() == TurnDepth::new(2).unwrap())
    });
    let cas_thread = source_cas_thread();
    let cas_turn = source_cas_turn();
    let cas_item = CasItemId::new(format!("source-user-{item}")).unwrap();
    let disposition = ProviderItemDisposition::CorrelatedUserInput {
        content,
        marker_count,
    };
    let descriptor = SourceItemDescriptor::new(
        item,
        cas_item.clone(),
        ProviderItemKind::UserMessage,
        disposition,
    )
    .unwrap();
    let source_authority = CasTurnSource::new(cas_thread.clone(), cas_turn.clone());
    records.extend([
        FixtureRecord::TurnState(fixture_turn_state(
            source,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            7,
            2,
            updated_at,
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(5).unwrap(),
                Some(source_authority.clone()),
                SourceEventPayload::ItemStarted {
                    item: descriptor.clone(),
                    assistant_phase: None,
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(6).unwrap(),
                Some(source_authority.clone()),
                SourceEventPayload::ItemCompleted {
                    item: descriptor,
                    assistant_phase: None,
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(7).unwrap(),
                Some(source_authority.clone()),
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_source_state(
                item,
                source,
                TurnItemOrdinal::new(2).unwrap(),
                revision,
                Some(SourceEventSequence::new(6).unwrap()),
                2,
                Some(CasItemSource::new(source_authority, cas_item.clone())),
                ProviderItemKind::UserMessage,
                ProviderItemLifecycle::Completed,
                disposition,
                None,
                CanonicalItemPayload::user_input(content, marker_count),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            source,
            TurnItemOrdinal::new(2).unwrap(),
            item,
            revision,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::FIRST,
            source,
            SourceEventSequence::new(5).unwrap(),
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::new(2).unwrap(),
            source,
            SourceEventSequence::new(6).unwrap(),
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_thread, cas_turn, cas_item, item, revision,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            source,
            source_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            7,
            2,
            2,
            updated_at,
        )),
    ]);
}

pub fn source_resource() -> SyndicResourceId {
    SyndicResourceId::from_bytes([35; 16])
}

pub fn source_resource_projection() -> SyndicProjectionId {
    SyndicProjectionId::from_bytes([34; 16])
}

pub fn active_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([42; 16])
}

pub fn active_item() -> SyndicItemId {
    SyndicItemId::from_bytes([43; 16])
}

pub fn active_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_inline_paragraph_projection(
        active_item(),
        active_turn(),
        "active",
    )
    .id()
}

pub fn suffix_item() -> SyndicItemId {
    SyndicItemId::from_bytes([60; 16])
}

pub fn build_item() -> SyndicItemId {
    SyndicItemId::from_bytes([61; 16])
}

pub fn suffix_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_empty_projection(suffix_item(), active_turn()).id()
}

pub fn active_snapshot() -> SyndicExecutionSnapshotId {
    SyndicExecutionSnapshotId::from_bytes([45; 16])
}

pub fn steering_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([46; 16])
}

pub fn next_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([47; 16])
}

pub fn cas_thread() -> CasThreadId {
    CasThreadId::new("populated-thread").unwrap()
}

pub fn cas_turn() -> CasTurnId {
    CasTurnId::new("populated-turn").unwrap()
}

pub fn cas_item() -> CasItemId {
    CasItemId::new("populated-item").unwrap()
}

fn execution_binding() -> beryl_model::ExecutionBinding {
    let path = beryl_model::RuntimeNativePath::from_admitted(
        beryl_model::RuntimeMode::host(),
        beryl_model::PathFlavor::Windows,
        "C:\\populated",
    )
    .unwrap();
    beryl_model::ExecutionBinding::new(
        beryl_model::RuntimeId::from_bytes([48; 16]),
        beryl_model::RootId::from_bytes([49; 16]),
        path,
    )
}

pub fn populated_records() -> Vec<FixtureRecord> {
    let source_thread = id(30);
    let source_draft = draft_id(31);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let source = source_turn();
    let root_digest = root_turn_chain_digest(root);
    let source_digest = child_turn_chain_digest(source, root, root_digest);
    let revision = ProjectionRevision::new(1).unwrap();
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let binding_one = BindingRevision::new(1).unwrap();
    let binding_two = BindingRevision::new(2).unwrap();
    let binding_three = BindingRevision::new(3).unwrap();
    let binding_four = BindingRevision::new(4).unwrap();
    let (empty_content, empty_content_records) =
        composer_content_records(&ComposerPayload::default());
    let (assistant_content, assistant_content_records) = utf8_content_records("assistant");
    let source_selected = SelectedPathProof::new(Some(source), thread_revision, source_digest);
    let represented_parent =
        CasRepresentedPrefixProof::new(Some(root), thread_revision, root_digest);
    let lineage = CasLineageProof::native(NativeCasLineage::Resume, represented_parent).unwrap();
    let source_cas_thread = source_cas_thread();
    let source_cas_turn = source_cas_turn();
    let source_cas_item = source_cas_item();
    let source_usable = UsableCasBinding::new(
        execution_binding(),
        source_cas_thread.clone(),
        represented_parent,
        CasNativeTurnCount::ZERO,
        test_tool_profile(),
        lineage,
    );
    let source_active = ActiveCasBinding::new(
        source_usable.clone(),
        source_snapshot(),
        source,
        InputGateRevision::new(1).unwrap(),
        timestamp(3),
    );
    let terminal_usable = UsableCasBinding::new(
        execution_binding(),
        source_cas_thread.clone(),
        CasRepresentedPrefixProof::new(Some(source), thread_revision, source_digest),
        CasNativeTurnCount::new(1),
        test_tool_profile(),
        lineage,
    );
    let source_descriptor = SourceItemDescriptor::new(
        source_item(),
        source_cas_item.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();

    let mut records = vec![
        FixtureRecord::Thread(ThreadRecord::new(
            source_thread,
            thread_revision,
            Some(source),
            source_draft,
            None,
            None,
            source_digest,
        )),
        FixtureRecord::Draft(DraftRecord::new(
            source_draft,
            source_thread,
            draft_revision,
            ConversationParent::Turn(source),
            None,
            None,
            empty_content,
            timestamp(1),
            timestamp(1),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            source_thread,
            source_draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(InputGateRecord::idle(source_thread)),
        FixtureRecord::Turn(TurnRecord::new(
            root,
            source_thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            root_digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            root,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            timestamp(2),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                root,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            source,
            source_thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(root),
            Some(root),
            TurnDepth::new(2).unwrap(),
            source_digest,
            timestamp(3),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            source,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            5,
            1,
            timestamp(4),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::FIRST,
                Some(CasTurnSource::new(
                    source_cas_thread.clone(),
                    source_cas_turn.clone(),
                )),
                SourceEventPayload::TurnActivated,
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(2).unwrap(),
                Some(CasTurnSource::new(
                    source_cas_thread.clone(),
                    source_cas_turn.clone(),
                )),
                SourceEventPayload::ItemStarted {
                    item: source_descriptor.clone(),
                    assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(3).unwrap(),
                Some(CasTurnSource::new(
                    source_cas_thread.clone(),
                    source_cas_turn.clone(),
                )),
                SourceEventPayload::ItemDelta {
                    item_id: source_item(),
                    cas_item_id: source_cas_item.clone(),
                    expected_kind: ProviderItemKind::AgentMessage,
                    text: SourceEventText::new("assistant").unwrap(),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(4).unwrap(),
                Some(CasTurnSource::new(
                    source_cas_thread.clone(),
                    source_cas_turn.clone(),
                )),
                SourceEventPayload::ItemCompleted {
                    item: source_descriptor,
                    assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(5).unwrap(),
                Some(CasTurnSource::new(
                    source_cas_thread.clone(),
                    source_cas_turn.clone(),
                )),
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            root,
            source,
            TurnDepth::new(2).unwrap(),
            source_digest,
        )),
    ];
    records.extend(empty_content_records);

    let item = source_item();
    let projection = source_projection();
    let projection_record = fixture_inline_paragraph_projection(item, source, "assistant");
    let resource = source_resource();
    let resource_projection = source_resource_projection();
    let item_generation = ItemProjectionGeneration::FIRST;
    let projection_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection,
        revision,
    );
    let projection_checkpoint = MarkdownParserCheckpoint::new(
        9,
        9,
        ContentPieceOrdinal::new(2).unwrap(),
        9,
        Box::<str>::default(),
        false,
        None,
    );
    let transcript_entry = TranscriptViewEntryRecord::new(
        source_thread,
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        item,
        revision,
        item_generation,
        projection,
        revision,
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &transcript_entry);
    records.extend([
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_source_state(
                item,
                source,
                TurnItemOrdinal::FIRST,
                revision,
                Some(SourceEventSequence::new(4).unwrap()),
                3,
                Some(CasItemSource::new(
                    CasTurnSource::new(source_cas_thread.clone(), source_cas_turn.clone()),
                    source_cas_item.clone(),
                )),
                ProviderItemKind::AgentMessage,
                ProviderItemLifecycle::Completed,
                ProviderItemDisposition::CanonicalText,
                Some(AssistantMessagePhase::FinalAnswer),
                CanonicalItemPayload::text(assistant_content),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            source,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::FIRST,
            source,
            SourceEventSequence::new(2).unwrap(),
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::new(2).unwrap(),
            source,
            SourceEventSequence::new(3).unwrap(),
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::new(3).unwrap(),
            source,
            SourceEventSequence::new(4).unwrap(),
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            source_cas_thread.clone(),
            source_cas_turn.clone(),
            source_cas_item,
            item,
            revision,
        )),
        FixtureRecord::Projection(projection_record),
        FixtureRecord::Projection(ProjectionRecord::new(
            resource_projection,
            revision,
            item,
            source,
            ProjectionOrdinal::new(2).unwrap(),
            ProjectionPayload::resource_reference(
                MarkdownBlockId::from_bytes([34; 32]),
                MarkdownBlockKind::FencedCode,
                ProjectionSourceRange::new(0, 9).unwrap(),
                resource,
                "assistant",
            )
            .unwrap(),
        )),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection,
            revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            item_generation,
            ProjectionFormatVersion::V1,
            revision,
            assistant_content,
            9,
            1,
            0,
            projection_digest,
            1,
            0,
            projection_digest,
            projection_checkpoint,
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            revision,
            item_generation,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::Resource(
            ResourceMetadataRecord::new(
                resource,
                revision,
                resource_projection,
                item,
                ResourceOrdinal::FIRST,
                ResourceKind::Attachment,
                "text/plain",
                ResourceBacking::CanonicalTextRange {
                    content_id: assistant_content.id(),
                    range: ProjectionSourceRange::new(0, 9).unwrap(),
                },
                [50; 32],
                Some(ProjectionSourceRange::new(0, 9).unwrap()),
                ResourceStructure::Opaque,
            )
            .unwrap(),
        ),
        FixtureRecord::ProjectionResource(ProjectionResourceIndexRecord::new(
            resource_projection,
            ResourceOrdinal::FIRST,
            resource,
            revision,
            [50; 32],
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            revision,
            1,
            Some(source),
            source_digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            revision,
            thread_revision,
            Some(source),
            source_digest,
            2,
            1,
            transcript_digest,
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            root,
            root_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            0,
            timestamp(2),
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            source,
            source_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            5,
            1,
            1,
            timestamp(4),
        )),
        FixtureRecord::TranscriptViewEntry(transcript_entry),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            source_thread,
            thread_revision,
            Some(source),
            source_digest,
            true,
            timestamp(4),
        )),
    ]);
    records.extend(assistant_content_records);

    records.extend([
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_one,
            source_selected,
            BindingState::unbound("source fixture").unwrap(),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_two,
            source_selected,
            BindingState::valid(source_usable.clone()),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_three,
            source_selected,
            BindingState::active(source_active),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_four,
            source_selected,
            BindingState::valid(terminal_usable),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            source_thread,
            binding_four,
            BindingLifecycle::Valid,
            source_digest,
        )),
        FixtureRecord::ExecutionSnapshot(ExecutionSnapshotRecord::new(
            source_snapshot(),
            source_thread,
            binding_three,
            InputGateRevision::new(1).unwrap(),
            source,
            source_cas_thread.clone(),
            source_selected,
            represented_parent,
            CasNativeTurnCount::ZERO,
            test_tool_profile(),
            lineage,
            execution_binding(),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            timestamp(3),
        )),
        FixtureRecord::ActiveCasTurn(ActiveCasTurnRecord::new(
            source_snapshot(),
            source_thread,
            source,
            binding_three,
            source_cas_thread.clone(),
            source_cas_turn.clone(),
            timestamp(3),
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
            source_cas_thread.clone(),
            source_thread,
            binding_two,
            binding_four,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread.clone(),
            source_thread,
            binding_two,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread.clone(),
            source_thread,
            binding_three,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread.clone(),
            source_thread,
            binding_four,
        )),
        FixtureRecord::CasTurn(CasTurnIndexRecord::new(
            source_cas_thread,
            source_cas_turn,
            source_thread,
            source,
            binding_three,
            source_snapshot(),
            CasNativeTurnCount::new(1),
        )),
    ]);

    records.extend(context::records());
    records.extend(active::records());
    records
}
