mod active;
mod bindings;
mod context;
mod provider;
mod seed;
mod source;
use super::{
    batch, canonical_empty_root_history_pair_for, commit, composer_content_records, draft_id,
    fixture_turn_state, id, seed_canonical_empty_thread, test_tool_profile, timestamp,
    utf8_content_records,
};
use beryl_model::{
    BindingRevision, CasItemId, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId, DiscussionContextOwnerId,
    DraftRevision, InputGateRevision, ProjectionRevision, SyndicAcceptedInputId, SyndicContentId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicTurnId,
    ThreadRevision,
};
use provider::{
    AgentItemFixtureState, ProviderItemFixture, ProviderSeedTurn, accept_clean, agent_item_fixture,
    command_item_fixture,
};
use seed::provider_command_owned;
pub use seed::seed_populated;
pub use source::*;
use source::{
    execution_binding, source_cas_item, source_cas_thread, source_cas_turn, source_snapshot,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

pub fn populated_records() -> Vec<FixtureRecord> {
    let source_thread = id(30);
    let source_draft = draft_id(31);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let source = source_turn();
    let root_digest = root_turn_chain_digest(root);
    let source_digest = child_turn_chain_digest(source, root, root_digest);
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let item_revision = ProjectionRevision::new(4).unwrap();
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let binding_one = BindingRevision::new(1).unwrap();
    let binding_two = BindingRevision::new(2).unwrap();
    let binding_three = BindingRevision::new(3).unwrap();
    let binding_four = BindingRevision::new(4).unwrap();
    let (_, empty_content_records) = composer_content_records(&ComposerPayload::default());
    // Retained unreferenced content is valid until the future garbage collector removes it.
    // Keep one exact text object so physical-family fixtures cover every chunked text family.
    let (_, retained_text_records) = utf8_content_records("assistant");
    let source_selected = SelectedPathProof::new(Some(source), thread_revision, source_digest);
    let represented_parent =
        CasRepresentedPrefixProof::new(Some(root), thread_revision, root_digest);
    let lineage = CasLineageProof::native(NativeCasLineage::Resume, represented_parent).unwrap();
    let source_cas_thread = source_cas_thread();
    let source_cas_turn = source_cas_turn();
    let source_cas_item = source_cas_item();
    let item = source_item();
    let source_authority = CasTurnSource::new(source_cas_thread.clone(), source_cas_turn.clone());
    let source_item_authority =
        CasItemSource::new(source_authority.clone(), source_cas_item.clone());
    let provider = agent_item_fixture(
        item,
        source,
        source_item_authority.clone(),
        SourceEventSequence::new(2).unwrap(),
        ProviderMessagePhaseV1::FinalAnswer,
        "assistant",
        AgentItemFixtureState::Finalized,
    );
    let assistant_source =
        ProjectionTextSource::provider_narrative(provider.canonical.narrative().unwrap());
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
    let source_execution = ThreadExecutionRecord::new(source_thread, execution_binding());
    let source_attributes = ThreadAttributesRecord::ordinary(source_thread);
    let source_usage = ThreadUsageRecord::empty(source_thread);
    let source_catalog = ThreadCatalogSummaryRecord::new(
        source_thread,
        ProjectionRevision::new(1).unwrap(),
        None,
        execution_binding(),
        ThreadArchiveState::Ordinary,
        timestamp(4),
        false,
        None,
        ThreadLineageDepth::FIRST,
        root_thread_lineage_digest(source_thread),
        ThreadCatalogSourceWitnesses::new(
            source_attributes.revision(),
            projection_revision,
            thread_revision,
            source_digest,
            thread_revision,
        ),
    );
    let mut records = vec![
        FixtureRecord::Thread(ThreadRecord::new(
            source_thread,
            source_selected,
            source_draft,
            ThreadLineageProof::new(
                None,
                None,
                syndic_storage::ThreadLineageDepth::FIRST,
                syndic_storage::root_thread_lineage_digest(source_thread),
            ),
            None,
        )),
        FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                source_thread,
                1,
                ImageLabelFrontier::EMPTY,
                ImageLabelFrontier::EMPTY,
            )
            .unwrap(),
        ),
        FixtureRecord::DraftImageLabelProtectionHead(
            DraftImageLabelProtectionHeadV1::new(source_thread, 1, ImageLabelFrontier::EMPTY)
                .unwrap(),
        ),
        FixtureRecord::ThreadExecution(source_execution),
        FixtureRecord::ThreadAttributes(source_attributes),
        FixtureRecord::ThreadUsage(source_usage),
        FixtureRecord::ThreadCatalogSummary(source_catalog),
        FixtureRecord::Draft(DraftRecord::new(
            source_draft,
            source_thread,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
            canonical_empty_root_history_pair_for(source_draft),
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
        FixtureRecord::ActivityQueryHead(ActivityQueryHeadRecord::empty(source_thread)),
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
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(provider.frames[0].clone()),
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
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(provider.frames[1].clone()),
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
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(provider.frames[2].clone()),
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
    let projection = source_projection();
    let projection_record = fixture_inline_paragraph_projection(item, source, "assistant");
    let resource = source_resource();
    let resource_projection = source_resource_projection();
    let item_generation = ItemProjectionGeneration::FIRST;
    let projection_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection,
        projection_revision,
    );
    let projection_checkpoint = MarkdownParserCheckpoint::new(
        9,
        9,
        ProjectionTextSourceCursor::ProviderNarrative { logical_start: 9 },
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
        item_revision,
        item_generation,
        projection,
        projection_revision,
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &transcript_entry);
    records.extend([
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_provider_state(
                item,
                source,
                TurnItemOrdinal::FIRST,
                item_revision,
                SourceEventSequence::new(4).unwrap(),
                3,
                source_item_authority,
                Some(AssistantMessagePhase::FinalAnswer),
                provider.canonical,
                Some(ProviderNarrativeCompletionDisposition::Equal),
                CanonicalItemPresentation::Narrative,
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            source,
            TurnItemOrdinal::FIRST,
            item,
            item_revision,
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
            item_revision,
        )),
        FixtureRecord::Projection(projection_record),
        FixtureRecord::Projection(ProjectionRecord::new(
            resource_projection,
            projection_revision,
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
            projection_revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            item_generation,
            ProjectionFormatVersion::V1,
            item_revision,
            assistant_source,
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
            projection_revision,
            item_revision,
            item_generation,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::Resource(
            ResourceMetadataRecord::new(
                resource,
                projection_revision,
                resource_projection,
                item,
                ResourceOrdinal::FIRST,
                ResourceKind::Attachment,
                "text/plain",
                ResourceBacking::TextRange {
                    source: assistant_source,
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
            projection_revision,
            [50; 32],
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            1,
            Some(source),
            source_digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            projection_revision,
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
            projection_revision,
            thread_revision,
            Some(source),
            source_digest,
            true,
            timestamp(4),
        )),
    ]);
    records.extend(bindings::records(
        source_thread,
        binding_one,
        binding_two,
        binding_three,
        binding_four,
        source_selected,
        source_usable,
        source_active,
        terminal_usable,
        source_digest,
        source,
        source_cas_thread,
        represented_parent,
        lineage,
        source_cas_turn,
    ));
    records.extend(context::records());
    records.extend(active::records());
    records.extend(retained_text_records);
    records.retain(|record| !provider_command_owned(record));
    records
}
