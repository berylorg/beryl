use super::*;
use beryl_model::{CasItemId, CasThreadId, CasTurnId, SyndicContentId, SyndicTurnId};
use syndic_storage::*;

pub(super) fn append(
    fixture: &Fixture,
    prior: Option<SealedProviderFrameReference>,
    text: &str,
    revision: u64,
    lines: u64,
) -> (ResourceMetadataRecord, SealedProviderFrameReference) {
    let item = SyndicItemId::from_bytes([11; 16]);
    let turn = SyndicTurnId::from_bytes([12; 16]);
    let source = CasItemSource::new(
        CasTurnSource::new(
            CasThreadId::new("anchor-thread").unwrap(),
            CasTurnId::new("anchor-turn").unwrap(),
        ),
        CasItemId::new("anchor-item").unwrap(),
    );
    let ordinal = prior
        .as_ref()
        .map_or(1, |prior| prior.frame().ordinal().get() + 1);
    let observation = if prior.is_none() {
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                text: ProviderTextV1::inline(text.to_owned()),
                phase: None,
                memory_citation: None,
            }),
        }
    } else {
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
            delta: ProviderTextV1::inline(text.to_owned()),
        })
    };
    let frame = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::new(ordinal).unwrap(),
        source.item_id().clone(),
        observation,
    );
    let event = SourceEventSequence::new(ordinal).unwrap();
    let plan = match prior {
        Some(prior) => ProviderFramePreparationPlan::subsequent(
            item,
            turn,
            source.clone(),
            event,
            prior,
            frame,
        ),
        None => ProviderFramePreparationPlan::first(
            item,
            turn,
            source.clone(),
            event,
            SyndicContentId::from_bytes([91; 16]),
            frame,
        ),
    };
    let prepared = prepare_provider_frame(plan).unwrap();
    let staged = stage_provider_frame(
        &prepared,
        prepared.initial_build().clone(),
        &mut |batch: &ProviderFrameStageBatch| {
            let mut records = FixtureBatch::new();
            for chunk in batch.chunks() {
                records
                    .put(FixtureRecord::ContentChunk(chunk.clone()))
                    .unwrap();
            }
            for span in batch.byte_spans() {
                records.put(FixtureRecord::ContentByteSpan(*span)).unwrap();
            }
            for span in batch.narrative_spans() {
                records
                    .put(FixtureRecord::ProviderNarrativeSpan(*span))
                    .unwrap();
            }
            let mut command =
                beryl_home_store::HomeCommand::new(fixture.home.home_revision().unwrap());
            command
                .add(fixture.storage.fixture_contribution(
                    fixture.storage.revision(&fixture.home).unwrap(),
                    records,
                ))
                .unwrap();
            fixture.home.execute(command)
        },
    )
    .unwrap();
    let ProviderFrameStageOutcome::Committed {
        value,
        later_failure: None,
        ..
    } = staged
    else {
        panic!("provider stage: {staged:?}")
    };
    let (target, manifest) =
        syndic_storage::test_faults::fixture_provider_content_manifest(item, value.target(), false);
    let source_text = ProjectionTextSource::provider_narrative(target.narrative().unwrap());
    let range = ProjectionSourceRange::new(0, source_text.logical_utf8_bytes()).unwrap();
    let projection = SyndicProjectionId::from_bytes([13; 16]);
    let resource = SyndicResourceId::from_bytes([17; 16]);
    let metadata = ResourceMetadataRecord::new(
        resource,
        ProjectionRevision::new(revision).unwrap(),
        projection,
        item,
        ResourceOrdinal::FIRST,
        syndic_storage::ResourceKind::Code,
        "text/plain",
        ResourceBacking::TextRange {
            source: source_text,
            range,
        },
        [22; 32],
        None,
        ResourceStructure::code(None, lines).unwrap(),
    )
    .unwrap();
    let mut records = FixtureBatch::new();
    records
        .put(FixtureRecord::ContentManifest(manifest))
        .unwrap();
    records
        .put(FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_provider_state(
                item,
                turn,
                TurnItemOrdinal::FIRST,
                ProjectionRevision::new(1).unwrap(),
                event,
                ordinal,
                source,
                Some(AssistantMessagePhase::Unknown),
                target.clone(),
                None,
                CanonicalItemPresentation::Narrative,
            )
            .unwrap(),
        ))
        .unwrap();
    records
        .put(FixtureRecord::Projection(ProjectionRecord::new(
            projection,
            ProjectionRevision::new(1).unwrap(),
            item,
            turn,
            ProjectionOrdinal::FIRST,
            ProjectionPayload::resource_reference(
                MarkdownBlockId::from_bytes([16; 32]),
                MarkdownBlockKind::FencedCode,
                range,
                resource,
                "resource preview",
            )
            .unwrap(),
        )))
        .unwrap();
    records
        .put(FixtureRecord::Resource(metadata.clone()))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), records),
    );
    (metadata, target)
}
