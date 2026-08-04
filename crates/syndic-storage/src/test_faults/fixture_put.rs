use beryl_home_store::MutationBuilder;

use crate::{codec::*, domain::SyndicDomain};

use super::{FixtureMutationError, FixtureRecord};

pub(super) fn put_record(
    builder: &mut MutationBuilder<'_, SyndicDomain>,
    record: &FixtureRecord,
) -> Result<(), FixtureMutationError> {
    match record {
        FixtureRecord::Thread(v) => builder.put::<ThreadsCodec>(&v.id(), v)?,
        FixtureRecord::ThreadExecution(v) => {
            builder.put::<ThreadExecutionsCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::ThreadAttributes(v) => {
            builder.put::<ThreadAttributesCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::ThreadUsage(v) => builder.put::<ThreadUsageCodec>(&v.thread_id(), v)?,
        FixtureRecord::ThreadCatalogSummary(v) => {
            builder.put::<ThreadCatalogSummariesCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::Draft(v) => builder.put::<DraftsCodec>(&v.id(), v)?,
        FixtureRecord::ContentManifest(v) => builder.put::<ContentManifestsCodec>(&v.id(), v)?,
        FixtureRecord::ContentChunk(v) => builder.put::<ContentChunksCodec>(
            &ContentChunkKey {
                owner: v.content_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ContentByteSpan(v) => builder.put::<ContentByteSpansCodec>(
            &ContentByteSpanKey {
                owner: v.content_id(),
                start: v.start(),
            },
            v,
        )?,
        FixtureRecord::ContentTextSpan(v) => builder.put::<ContentTextSpansCodec>(
            &ContentTextSpanKey {
                owner: v.content_id(),
                logical_start: v.logical_start(),
            },
            v,
        )?,
        FixtureRecord::ContentPiece(v) => builder.put::<ContentPiecesCodec>(
            &ContentPieceKey {
                owner: v.content_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ProviderNarrativeSpan(v) => builder.put::<ProviderNarrativeSpansCodec>(
            &ProviderNarrativeSpanKey::new(v.content_id(), v.generation(), v.logical_start()),
            v,
        )?,
        FixtureRecord::ContextEnvelope(v) => {
            builder.put::<ContextEnvelopesCodec>(&ContextOwnerKey::from(v.owner()), v)?
        }
        FixtureRecord::Turn(v) => builder.put::<TurnsCodec>(&v.id(), v)?,
        FixtureRecord::TurnState(v) => builder.put::<TurnStatesCodec>(&v.turn_id(), v)?,
        FixtureRecord::InputGate(v) => builder.put::<InputGatesCodec>(&v.thread_id(), v)?,
        FixtureRecord::AcceptedInput(v) => builder.put::<AcceptedInputsCodec>(&v.id(), v)?,
        FixtureRecord::StopOperation(v) => builder.put::<StopOperationsCodec>(&v.id(), v)?,
        FixtureRecord::CompactionOperation(v) => {
            builder.put::<CompactionOperationsCodec>(&v.id(), v)?
        }
        FixtureRecord::CompactionSettlementReceipt(v) => {
            builder.put::<CompactionSettlementReceiptsCodec>(&v.operation_id(), v)?
        }
        FixtureRecord::AcceptedRouteGenerationHead(v) => {
            builder.put::<AcceptedRouteGenerationHeadsCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::AcceptedRouteLeaf(v) => {
            builder.put::<AcceptedRouteLeavesCodec>(&v.input_id(), v)?
        }
        FixtureRecord::SourceEvent(v) => builder.put::<SourceEventsCodec>(
            &TurnEventKey {
                owner: v.turn_id(),
                ordinal: v.sequence(),
            },
            v,
        )?,
        FixtureRecord::CanonicalItem(v) => builder.put::<CanonicalItemsCodec>(&v.id(), v)?,
        FixtureRecord::ActivityQueryHead(v) => {
            builder.put::<ActivityQueryHeadsCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::ItemProjectionHead(v) => {
            builder.put::<ItemProjectionHeadsCodec>(&v.item_id(), v)?
        }
        FixtureRecord::ItemProjectionSet(v) => builder.put::<ItemProjectionSetsCodec>(
            &ItemProjectionSetKey {
                item: v.item_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::ItemProjectionBuild(v) => builder.put::<ItemProjectionBuildsCodec>(
            &ItemProjectionSetKey {
                item: v.item_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::TranscriptViewHead(v) => {
            builder.put::<TranscriptHeadsCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::TranscriptBuild(v) => builder.put::<TranscriptBuildsCodec>(
            &ThreadTranscriptBuildKey {
                thread: v.thread_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::Projection(v) => builder.put::<ProjectionsCodec>(&v.id(), v)?,
        FixtureRecord::Resource(v) => builder.put::<ResourcesCodec>(&v.id(), v)?,
        FixtureRecord::HistorySummary(v) => {
            builder.put::<HistorySummariesCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::Binding(v) => builder.put::<BindingsCodec>(
            &BindingKey {
                thread: v.thread_id(),
                revision: v.revision(),
            },
            v,
        )?,
        FixtureRecord::ExecutionSnapshot(v) => {
            builder.put::<ExecutionSnapshotsCodec>(&v.id(), v)?
        }
        FixtureRecord::ActiveCasTurn(v) => {
            builder.put::<ActiveCasTurnsCodec>(&v.snapshot_id(), v)?
        }
        FixtureRecord::DraftByThread(v) => builder.put::<DraftByThreadCodec>(&v.thread_id(), v)?,
        FixtureRecord::ThreadParent(v) => builder.put::<ThreadParentCodec>(
            &ThreadPairKey {
                first: v.parent_thread_id(),
                second: v.child_thread_id(),
            },
            v,
        )?,
        FixtureRecord::ImageLabelOriginSpan(v) => builder.put::<ImageLabelOriginSpansCodec>(
            &ImageLabelOriginSpanKey {
                thread: v.thread_id(),
                end_label: v.end_label(),
            },
            v,
        )?,
        FixtureRecord::TurnChild(v) => builder.put::<TurnChildrenCodec>(
            &TurnPairKey {
                parent: v.parent_id(),
                child: v.child_id(),
            },
            v,
        )?,
        FixtureRecord::AcceptedOrder(v) => builder.put::<AcceptedOrderCodec>(
            &ThreadAcceptedKey {
                owner: v.thread_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::AcceptedRouteGeneration(v) => builder.put::<AcceptedRouteGenerationsCodec>(
            &ThreadRouteKey {
                thread: v.thread_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::AcceptedReadySource(v) => builder.put::<AcceptedReadySourcesCodec>(
            &ThreadRouteKey {
                thread: v.thread_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::AcceptedNextSource(v) => builder.put::<AcceptedNextSourcesCodec>(
            &ThreadRouteKey {
                thread: v.thread_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::TurnItem(v) => builder.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: v.turn_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ActivityQueryEntry(v) => builder.put::<ActivityQueryEntriesCodec>(
            &ActivityQueryEntryKey {
                thread: v.thread_id(),
                work_period: v.work_period(),
                order: v.order(),
            },
            v,
        )?,
        FixtureRecord::ActivityQuerySource(v) => builder.put::<ActivityQuerySourcesCodec>(
            &ActivityQuerySourceKey {
                thread: v.thread_id(),
                work_period: v.work_period(),
                source_thread: v.source().thread_id(),
                source_turn: v.source().turn_id(),
            },
            v,
        )?,
        FixtureRecord::ItemSourceEvent(v) => builder.put::<ItemSourceEventsCodec>(
            &ItemEventKey {
                owner: v.item_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::CasItem(v) => builder.put::<CasItemIndexCodec>(
            &CasItemKey::Record(
                v.cas_thread_id.clone(),
                v.cas_turn_id.clone(),
                v.cas_item_id.clone(),
            ),
            v,
        )?,
        FixtureRecord::TranscriptPathTurn(v) => builder.put::<TranscriptPathTurnsCodec>(
            &ThreadTranscriptPathKey {
                thread: v.thread_id(),
                generation: v.generation(),
                depth: v.depth(),
            },
            v,
        )?,
        FixtureRecord::TranscriptViewEntry(v) => builder.put::<TranscriptEntriesCodec>(
            &ThreadTranscriptKey {
                thread: v.thread_id(),
                generation: v.generation(),
                position: v.position(),
            },
            v,
        )?,
        FixtureRecord::StableItemProjection(v) => builder.put::<StableItemProjectionsCodec>(
            &StableItemProjectionKey {
                item: v.item_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ItemProjection(v) => builder.put::<ItemProjectionsCodec>(
            &ItemProjectionKey {
                item: v.item_id,
                generation: v.generation,
                ordinal: v.ordinal,
            },
            v,
        )?,
        FixtureRecord::ProjectionResource(v) => builder.put::<ProjectionResourcesCodec>(
            &ProjectionResourceKey {
                owner: v.projection_id,
                ordinal: v.ordinal,
            },
            v,
        )?,
        FixtureRecord::BindingHead(v) => builder.put::<BindingHeadsCodec>(&v.thread_id(), v)?,
        FixtureRecord::CasThread(v) => builder
            .put::<CasThreadIndexCodec>(&CasThreadKey::Record(v.cas_thread_id().clone()), v)?,
        FixtureRecord::CasThreadBinding(v) => builder.put::<CasThreadBindingIndexCodec>(
            &CasThreadBindingKey::Record(v.cas_thread_id().clone(), v.binding_revision()),
            v,
        )?,
        FixtureRecord::CasTurn(v) => builder.put::<CasTurnIndexCodec>(
            &CasTurnKey::Record(v.cas_thread_id.clone(), v.cas_turn_id.clone()),
            v,
        )?,
    }
    Ok(())
}
