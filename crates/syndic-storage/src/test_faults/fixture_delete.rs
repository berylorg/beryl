use beryl_home_store::MutationBuilder;

use crate::{codec::*, domain::SyndicDomain};

use super::{FixtureDelete, FixtureMutationError};

pub(super) fn delete_record(
    builder: &mut MutationBuilder<'_, SyndicDomain>,
    key: &FixtureDelete,
) -> Result<(), FixtureMutationError> {
    match key {
        FixtureDelete::Thread(v) => builder.delete::<ThreadsCodec>(v)?,
        FixtureDelete::ImageLabelAuthorityHead(v) => {
            builder.delete::<ImageLabelAuthorityHeadsCodec>(v)?
        }
        FixtureDelete::DraftImageLabelProtectionHead(v) => {
            builder.delete::<DraftImageLabelProtectionHeadsCodec>(v)?
        }
        FixtureDelete::ThreadExecution(v) => builder.delete::<ThreadExecutionsCodec>(v)?,
        FixtureDelete::ThreadAttributes(v) => builder.delete::<ThreadAttributesCodec>(v)?,
        FixtureDelete::ThreadUsage(v) => builder.delete::<ThreadUsageCodec>(v)?,
        FixtureDelete::ThreadCatalogSummary(v) => {
            builder.delete::<ThreadCatalogSummariesCodec>(v)?
        }
        FixtureDelete::Draft(v) => builder.delete::<DraftsCodec>(v)?,
        FixtureDelete::ContentManifest(v) => builder.delete::<ContentManifestsCodec>(v)?,
        FixtureDelete::ContentChunk { content, ordinal } => {
            builder.delete::<ContentChunksCodec>(&ContentChunkKey {
                owner: *content,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ContentByteSpan { content, start } => builder
            .delete::<ContentByteSpansCodec>(&ContentByteSpanKey {
                owner: *content,
                start: *start,
            })?,
        FixtureDelete::ContentTextSpan {
            content,
            logical_start,
        } => builder.delete::<ContentTextSpansCodec>(&ContentTextSpanKey {
            owner: *content,
            logical_start: *logical_start,
        })?,
        FixtureDelete::ContentPiece { content, ordinal } => {
            builder.delete::<ContentPiecesCodec>(&ContentPieceKey {
                owner: *content,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ProviderNarrativeSpan {
            content,
            generation,
            logical_start,
        } => builder.delete::<ProviderNarrativeSpansCodec>(&ProviderNarrativeSpanKey::new(
            *content,
            *generation,
            *logical_start,
        ))?,
        FixtureDelete::ContextEnvelope(v) => {
            builder.delete::<ContextEnvelopesCodec>(&ContextOwnerKey::from(*v))?
        }
        FixtureDelete::Turn(v) => builder.delete::<TurnsCodec>(v)?,
        FixtureDelete::TurnState(v) => builder.delete::<TurnStatesCodec>(v)?,
        FixtureDelete::InputGate(v) => builder.delete::<InputGatesCodec>(v)?,
        FixtureDelete::AcceptedInput(v) => builder.delete::<AcceptedInputsCodec>(v)?,
        FixtureDelete::StopOperation(v) => builder.delete::<StopOperationsCodec>(v)?,
        FixtureDelete::CompactionOperation(v) => builder.delete::<CompactionOperationsCodec>(v)?,
        FixtureDelete::CompactionSettlementReceipt(v) => {
            builder.delete::<CompactionSettlementReceiptsCodec>(v)?
        }
        FixtureDelete::AcceptedRouteGenerationHead(v) => {
            builder.delete::<AcceptedRouteGenerationHeadsCodec>(v)?
        }
        FixtureDelete::AcceptedRouteLeaf(v) => builder.delete::<AcceptedRouteLeavesCodec>(v)?,
        FixtureDelete::SourceEvent { turn, sequence } => {
            builder.delete::<SourceEventsCodec>(&TurnEventKey {
                owner: *turn,
                ordinal: *sequence,
            })?
        }
        FixtureDelete::CanonicalItem(v) => builder.delete::<CanonicalItemsCodec>(v)?,
        FixtureDelete::ActivityQueryHead(v) => builder.delete::<ActivityQueryHeadsCodec>(v)?,
        FixtureDelete::ItemProjectionHead(v) => builder.delete::<ItemProjectionHeadsCodec>(v)?,
        FixtureDelete::ItemProjectionSet { item, generation } => builder
            .delete::<ItemProjectionSetsCodec>(&ItemProjectionSetKey {
                item: *item,
                generation: *generation,
            })?,
        FixtureDelete::ItemProjectionBuild { item, generation } => {
            builder.delete::<ItemProjectionBuildsCodec>(&ItemProjectionSetKey {
                item: *item,
                generation: *generation,
            })?
        }
        FixtureDelete::TranscriptViewHead(v) => builder.delete::<TranscriptHeadsCodec>(v)?,
        FixtureDelete::TranscriptBuild { thread, generation } => builder
            .delete::<TranscriptBuildsCodec>(&ThreadTranscriptBuildKey {
                thread: *thread,
                generation: *generation,
            })?,
        FixtureDelete::Projection(v) => builder.delete::<ProjectionsCodec>(v)?,
        FixtureDelete::Resource(v) => builder.delete::<ResourcesCodec>(v)?,
        FixtureDelete::HistorySummary(v) => builder.delete::<HistorySummariesCodec>(v)?,
        FixtureDelete::Binding { thread, revision } => {
            builder.delete::<BindingsCodec>(&BindingKey {
                thread: *thread,
                revision: *revision,
            })?
        }
        FixtureDelete::ExecutionSnapshot(v) => builder.delete::<ExecutionSnapshotsCodec>(v)?,
        FixtureDelete::ActiveCasTurn(v) => builder.delete::<ActiveCasTurnsCodec>(v)?,
        FixtureDelete::DraftByThread(v) => builder.delete::<DraftByThreadCodec>(v)?,
        FixtureDelete::ThreadParent { parent, child } => {
            builder.delete::<ThreadParentCodec>(&ThreadPairKey {
                first: *parent,
                second: *child,
            })?
        }
        FixtureDelete::ImageLabelOriginSpan { thread, end_label } => {
            builder.delete::<ImageLabelOriginSpansCodec>(&ImageLabelOriginSpanKey {
                thread: *thread,
                end_label: *end_label,
            })?
        }
        FixtureDelete::TurnChild { parent, child } => {
            builder.delete::<TurnChildrenCodec>(&TurnPairKey {
                parent: *parent,
                child: *child,
            })?
        }
        FixtureDelete::AcceptedOrder { thread, ordinal } => {
            builder.delete::<AcceptedOrderCodec>(&ThreadAcceptedKey {
                owner: *thread,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::AcceptedRouteGeneration { thread, generation } => {
            builder.delete::<AcceptedRouteGenerationsCodec>(&ThreadRouteKey {
                thread: *thread,
                generation: *generation,
            })?
        }
        FixtureDelete::AcceptedReadySource { thread, generation } => {
            builder.delete::<AcceptedReadySourcesCodec>(&ThreadRouteKey {
                thread: *thread,
                generation: *generation,
            })?
        }
        FixtureDelete::AcceptedNextSource { thread, generation } => {
            builder.delete::<AcceptedNextSourcesCodec>(&ThreadRouteKey {
                thread: *thread,
                generation: *generation,
            })?
        }
        FixtureDelete::TurnItem { turn, ordinal } => {
            builder.delete::<TurnItemsCodec>(&TurnItemKey {
                owner: *turn,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ActivityQueryEntry {
            thread,
            work_period,
            order,
        } => builder.delete::<ActivityQueryEntriesCodec>(&ActivityQueryEntryKey {
            thread: *thread,
            work_period: *work_period,
            order: *order,
        })?,
        FixtureDelete::ItemSourceEvent { item, ordinal } => builder
            .delete::<ItemSourceEventsCodec>(&ItemEventKey {
                owner: *item,
                ordinal: *ordinal,
            })?,
        FixtureDelete::CasItem { thread, turn, item } => builder.delete::<CasItemIndexCodec>(
            &CasItemKey::Record(thread.clone(), turn.clone(), item.clone()),
        )?,
        FixtureDelete::TranscriptPathTurn {
            thread,
            generation,
            depth,
        } => builder.delete::<TranscriptPathTurnsCodec>(&ThreadTranscriptPathKey {
            thread: *thread,
            generation: *generation,
            depth: *depth,
        })?,
        FixtureDelete::ActivityQuerySource {
            thread,
            work_period,
            source_thread,
            source_turn,
        } => builder.delete::<ActivityQuerySourcesCodec>(&ActivityQuerySourceKey {
            thread: *thread,
            work_period: *work_period,
            source_thread: *source_thread,
            source_turn: *source_turn,
        })?,
        FixtureDelete::TranscriptViewEntry {
            thread,
            generation,
            position,
        } => builder.delete::<TranscriptEntriesCodec>(&ThreadTranscriptKey {
            thread: *thread,
            generation: *generation,
            position: *position,
        })?,
        FixtureDelete::StableItemProjection { item, ordinal } => {
            builder.delete::<StableItemProjectionsCodec>(&StableItemProjectionKey {
                item: *item,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ItemProjection {
            item,
            generation,
            ordinal,
        } => builder.delete::<ItemProjectionsCodec>(&ItemProjectionKey {
            item: *item,
            generation: *generation,
            ordinal: *ordinal,
        })?,
        FixtureDelete::ProjectionResource {
            projection,
            ordinal,
        } => builder.delete::<ProjectionResourcesCodec>(&ProjectionResourceKey {
            owner: *projection,
            ordinal: *ordinal,
        })?,
        FixtureDelete::BindingHead(v) => builder.delete::<BindingHeadsCodec>(v)?,
        FixtureDelete::CasThread(v) => {
            builder.delete::<CasThreadIndexCodec>(&CasThreadKey::Record(v.clone()))?
        }
        FixtureDelete::CasThreadBinding { thread, revision } => builder
            .delete::<CasThreadBindingIndexCodec>(
            &CasThreadBindingKey::Record(thread.clone(), *revision),
        )?,
        FixtureDelete::CasTurn { thread, turn } => builder
            .delete::<CasTurnIndexCodec>(&CasTurnKey::Record(thread.clone(), turn.clone()))?,
    }
    Ok(())
}
