use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, MutationBuildError, MutationBuilder, MutationContribution,
};
use beryl_model::DomainRevision;

use crate::{codec::*, domain::SyndicDomain, *};

use super::{FixtureDelete, FixtureRecord, fixture_delete::delete_record, fixture_put::put_record};

const MAX_FIXTURE_OPERATIONS: usize = 131_072;

#[derive(Clone, Debug)]
enum FixtureOperation {
    Put(Box<FixtureRecord>),
    Delete(FixtureDelete),
}

impl FixtureOperation {
    const fn family(&self) -> super::PhysicalFamily {
        match self {
            Self::Put(record) => record.family(),
            Self::Delete(key) => match key {
                FixtureDelete::Thread(_) => super::PhysicalFamily::Threads,
                FixtureDelete::ImageLabelAuthorityHead(_) => {
                    super::PhysicalFamily::ImageLabelAuthorityHeads
                }
                FixtureDelete::DraftImageLabelProtectionHead(_) => {
                    super::PhysicalFamily::DraftImageLabelProtectionHeads
                }
                FixtureDelete::ThreadExecution(_) => super::PhysicalFamily::ThreadExecutions,
                FixtureDelete::ThreadAttributes(_) => super::PhysicalFamily::ThreadAttributes,
                FixtureDelete::ThreadUsage(_) => super::PhysicalFamily::ThreadUsage,
                FixtureDelete::ThreadCatalogSummary(_) => {
                    super::PhysicalFamily::ThreadCatalogSummaries
                }
                FixtureDelete::Draft(_) => super::PhysicalFamily::Drafts,
                FixtureDelete::ContentManifest(_) => super::PhysicalFamily::ContentManifests,
                FixtureDelete::ContentChunk { .. } => super::PhysicalFamily::ContentChunks,
                FixtureDelete::ContentByteSpan { .. } => super::PhysicalFamily::ContentByteSpans,
                FixtureDelete::ContentTextSpan { .. } => super::PhysicalFamily::ContentTextSpans,
                FixtureDelete::ContentPiece { .. } => super::PhysicalFamily::ContentPieces,
                FixtureDelete::ProviderNarrativeSpan { .. } => {
                    super::PhysicalFamily::ProviderNarrativeSpans
                }
                FixtureDelete::ContextEnvelope(_) => super::PhysicalFamily::ContextEnvelopes,
                FixtureDelete::Turn(_) => super::PhysicalFamily::Turns,
                FixtureDelete::TurnState(_) => super::PhysicalFamily::TurnStates,
                FixtureDelete::InputGate(_) => super::PhysicalFamily::InputGates,
                FixtureDelete::AcceptedInput(_) => super::PhysicalFamily::AcceptedInputs,
                FixtureDelete::StopOperation(_) => super::PhysicalFamily::StopOperations,
                FixtureDelete::CompactionOperation(_) => {
                    super::PhysicalFamily::CompactionOperations
                }
                FixtureDelete::CompactionSettlementReceipt(_) => {
                    super::PhysicalFamily::CompactionSettlementReceipts
                }
                FixtureDelete::AcceptedRouteGenerationHead(_) => {
                    super::PhysicalFamily::AcceptedRouteGenerationHeads
                }
                FixtureDelete::AcceptedRouteLeaf(_) => super::PhysicalFamily::AcceptedRouteLeaves,
                FixtureDelete::SourceEvent { .. } => super::PhysicalFamily::SourceEvents,
                FixtureDelete::CanonicalItem(_) => super::PhysicalFamily::CanonicalItems,
                FixtureDelete::ActivityQueryHead(_) => super::PhysicalFamily::ActivityQueryHeads,
                FixtureDelete::ItemProjectionHead(_) => super::PhysicalFamily::ItemProjectionHeads,
                FixtureDelete::ItemProjectionSet { .. } => {
                    super::PhysicalFamily::ItemProjectionSets
                }
                FixtureDelete::ItemProjectionBuild { .. } => {
                    super::PhysicalFamily::ItemProjectionBuilds
                }
                FixtureDelete::TranscriptViewHead(_) => super::PhysicalFamily::TranscriptViewHeads,
                FixtureDelete::TranscriptBuild { .. } => super::PhysicalFamily::TranscriptBuilds,
                FixtureDelete::Projection(_) => super::PhysicalFamily::Projections,
                FixtureDelete::Resource(_) => super::PhysicalFamily::Resources,
                FixtureDelete::HistorySummary(_) => super::PhysicalFamily::HistorySummaries,
                FixtureDelete::Binding { .. } => super::PhysicalFamily::Bindings,
                FixtureDelete::ExecutionSnapshot(_) => super::PhysicalFamily::ExecutionSnapshots,
                FixtureDelete::ActiveCasTurn(_) => super::PhysicalFamily::ActiveCasTurns,
                FixtureDelete::DraftByThread(_) => super::PhysicalFamily::DraftByThread,
                FixtureDelete::ThreadParent { .. } => super::PhysicalFamily::ThreadParent,
                FixtureDelete::ImageLabelOriginSpan { .. } => {
                    super::PhysicalFamily::ImageLabelOriginSpans
                }
                FixtureDelete::TurnChild { .. } => super::PhysicalFamily::TurnChildren,
                FixtureDelete::AcceptedOrder { .. } => super::PhysicalFamily::AcceptedOrder,
                FixtureDelete::AcceptedRouteGeneration { .. } => {
                    super::PhysicalFamily::AcceptedRouteGenerations
                }
                FixtureDelete::AcceptedReadySource { .. } => {
                    super::PhysicalFamily::AcceptedReadySources
                }
                FixtureDelete::AcceptedNextSource { .. } => {
                    super::PhysicalFamily::AcceptedNextSources
                }
                FixtureDelete::TurnItem { .. } => super::PhysicalFamily::TurnItems,
                FixtureDelete::ActivityQueryEntry { .. } => {
                    super::PhysicalFamily::ActivityQueryEntries
                }
                FixtureDelete::ActivityQuerySource { .. } => {
                    super::PhysicalFamily::ActivityQuerySources
                }
                FixtureDelete::ItemSourceEvent { .. } => super::PhysicalFamily::ItemSourceEvents,
                FixtureDelete::CasItem { .. } => super::PhysicalFamily::CasItem,
                FixtureDelete::TranscriptPathTurn { .. } => {
                    super::PhysicalFamily::TranscriptPathTurns
                }
                FixtureDelete::TranscriptViewEntry { .. } => {
                    super::PhysicalFamily::TranscriptViewEntries
                }
                FixtureDelete::StableItemProjection { .. } => {
                    super::PhysicalFamily::StableItemProjections
                }
                FixtureDelete::ItemProjection { .. } => super::PhysicalFamily::ItemProjections,
                FixtureDelete::ProjectionResource { .. } => {
                    super::PhysicalFamily::ProjectionResources
                }
                FixtureDelete::BindingHead(_) => super::PhysicalFamily::BindingHeads,
                FixtureDelete::CasThread(_) => super::PhysicalFamily::CasThread,
                FixtureDelete::CasThreadBinding { .. } => super::PhysicalFamily::CasThreadBinding,
                FixtureDelete::CasTurn { .. } => super::PhysicalFamily::CasTurn,
            },
        }
    }
}

/// One bounded exact-domain batch used to seed valid or intentionally inconsistent fixtures.
#[derive(Clone, Debug, Default)]
pub struct FixtureBatch {
    operations: Vec<FixtureOperation>,
}

impl FixtureBatch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, record: FixtureRecord) -> Result<&mut Self, FixtureBuildError> {
        self.push(FixtureOperation::Put(Box::new(record)))?;
        Ok(self)
    }

    pub fn delete(&mut self, key: FixtureDelete) -> Result<&mut Self, FixtureBuildError> {
        self.push(FixtureOperation::Delete(key))?;
        Ok(self)
    }

    fn push(&mut self, operation: FixtureOperation) -> Result<(), FixtureBuildError> {
        if self.operations.len() == MAX_FIXTURE_OPERATIONS {
            return Err(FixtureBuildError::TooManyOperations);
        }
        self.operations.push(operation);
        Ok(())
    }
}

/// Why a test-only typed fixture batch could not be built.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FixtureBuildError {
    #[error("Syndic fixture exceeds its fixed operation bound")]
    TooManyOperations,
}

#[derive(Debug)]
pub enum FixtureMutationError {
    Build(MutationBuildError),
}

impl fmt::Display for FixtureMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(source) => source.fmt(f),
        }
    }
}

impl Error for FixtureMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(source) => Some(source),
        }
    }
}

impl From<MutationBuildError> for FixtureMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl DomainCallbackError for FixtureMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

impl DomainMutation<SyndicDomain> for FixtureBatch {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let mut quotas = Vec::<(super::PhysicalFamily, usize)>::new();
        for operation in &self.operations {
            let family = operation.family();
            if let Some((_, count)) = quotas.iter_mut().find(|(known, _)| *known == family) {
                *count = count.checked_add(1).ok_or(
                    MutationBuildError::ReconciliationReservationOverflow {
                        domain: "syndic",
                        family: family.name(),
                    },
                )?;
            } else {
                quotas.push((family, 1));
            }
        }
        for (family, count) in quotas {
            reserve_fixture_family(reservation, family, count)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for operation in &prepared.operations {
            match operation {
                FixtureOperation::Put(record) => put_record(builder, record.as_ref())?,
                FixtureOperation::Delete(key) => delete_record(builder, key)?,
            }
        }
        Ok(())
    }
}

fn reserve_fixture_family(
    reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    family: super::PhysicalFamily,
    count: usize,
) -> Result<(), FixtureMutationError> {
    macro_rules! reserve {
        ($codec:ty) => {
            reservation.reserve_records::<$codec>(count)?
        };
    }

    match family {
        super::PhysicalFamily::Threads => reserve!(ThreadsCodec),
        super::PhysicalFamily::ImageLabelAuthorityHeads => {
            reserve!(ImageLabelAuthorityHeadsCodec)
        }
        super::PhysicalFamily::DraftImageLabelProtectionHeads => {
            reserve!(DraftImageLabelProtectionHeadsCodec)
        }
        super::PhysicalFamily::ThreadExecutions => reserve!(ThreadExecutionsCodec),
        super::PhysicalFamily::ThreadAttributes => reserve!(ThreadAttributesCodec),
        super::PhysicalFamily::ThreadUsage => reserve!(ThreadUsageCodec),
        super::PhysicalFamily::ThreadCatalogSummaries => reserve!(ThreadCatalogSummariesCodec),
        super::PhysicalFamily::Drafts => reserve!(DraftsCodec),
        super::PhysicalFamily::ContentManifests => reserve!(ContentManifestsCodec),
        super::PhysicalFamily::ContentChunks => reserve!(ContentChunksCodec),
        super::PhysicalFamily::ContentByteSpans => reserve!(ContentByteSpansCodec),
        super::PhysicalFamily::ContentTextSpans => reserve!(ContentTextSpansCodec),
        super::PhysicalFamily::ContentPieces => reserve!(ContentPiecesCodec),
        super::PhysicalFamily::ProviderNarrativeSpans => reserve!(ProviderNarrativeSpansCodec),
        super::PhysicalFamily::ProviderItemBuilds => reserve!(ProviderItemBuildsCodec),
        super::PhysicalFamily::ProviderObservationBuilds => {
            reserve!(ProviderObservationBuildsCodec)
        }
        super::PhysicalFamily::ProviderObservationChunks => {
            reserve!(ProviderObservationChunksCodec)
        }
        super::PhysicalFamily::ContextEnvelopes => reserve!(ContextEnvelopesCodec),
        super::PhysicalFamily::Turns => reserve!(TurnsCodec),
        super::PhysicalFamily::TurnStates => reserve!(TurnStatesCodec),
        super::PhysicalFamily::InputGates => reserve!(InputGatesCodec),
        super::PhysicalFamily::AcceptedInputs => reserve!(AcceptedInputsCodec),
        super::PhysicalFamily::StopOperations => reserve!(StopOperationsCodec),
        super::PhysicalFamily::CompactionOperations => reserve!(CompactionOperationsCodec),
        super::PhysicalFamily::CompactionSettlementReceipts => {
            reserve!(CompactionSettlementReceiptsCodec)
        }
        super::PhysicalFamily::AcceptedRouteGenerationHeads => {
            reserve!(AcceptedRouteGenerationHeadsCodec)
        }
        super::PhysicalFamily::AcceptedRouteLeaves => reserve!(AcceptedRouteLeavesCodec),
        super::PhysicalFamily::SourceEvents => reserve!(SourceEventsCodec),
        super::PhysicalFamily::CanonicalItems => reserve!(CanonicalItemsCodec),
        super::PhysicalFamily::ActivityQueryHeads => reserve!(ActivityQueryHeadsCodec),
        super::PhysicalFamily::ItemProjectionHeads => reserve!(ItemProjectionHeadsCodec),
        super::PhysicalFamily::ItemProjectionSets => reserve!(ItemProjectionSetsCodec),
        super::PhysicalFamily::ItemProjectionBuilds => reserve!(ItemProjectionBuildsCodec),
        super::PhysicalFamily::TranscriptViewHeads => reserve!(TranscriptHeadsCodec),
        super::PhysicalFamily::TranscriptBuilds => reserve!(TranscriptBuildsCodec),
        super::PhysicalFamily::Projections => reserve!(ProjectionsCodec),
        super::PhysicalFamily::Resources => reserve!(ResourcesCodec),
        super::PhysicalFamily::HistorySummaries => reserve!(HistorySummariesCodec),
        super::PhysicalFamily::Bindings => reserve!(BindingsCodec),
        super::PhysicalFamily::ExecutionSnapshots => reserve!(ExecutionSnapshotsCodec),
        super::PhysicalFamily::ActiveCasTurns => reserve!(ActiveCasTurnsCodec),
        super::PhysicalFamily::DraftByThread => reserve!(DraftByThreadCodec),
        super::PhysicalFamily::ThreadParent => reserve!(ThreadParentCodec),
        super::PhysicalFamily::ImageLabelOriginSpans => reserve!(ImageLabelOriginSpansCodec),
        super::PhysicalFamily::TurnChildren => reserve!(TurnChildrenCodec),
        super::PhysicalFamily::AcceptedOrder => reserve!(AcceptedOrderCodec),
        super::PhysicalFamily::AcceptedRouteGenerations => reserve!(AcceptedRouteGenerationsCodec),
        super::PhysicalFamily::AcceptedReadySources => reserve!(AcceptedReadySourcesCodec),
        super::PhysicalFamily::AcceptedNextSources => reserve!(AcceptedNextSourcesCodec),
        super::PhysicalFamily::TurnItems => reserve!(TurnItemsCodec),
        super::PhysicalFamily::ActivityQueryEntries => reserve!(ActivityQueryEntriesCodec),
        super::PhysicalFamily::ActivityQuerySources => reserve!(ActivityQuerySourcesCodec),
        super::PhysicalFamily::ItemSourceEvents => reserve!(ItemSourceEventsCodec),
        super::PhysicalFamily::CasItem => reserve!(CasItemIndexCodec),
        super::PhysicalFamily::TranscriptPathTurns => reserve!(TranscriptPathTurnsCodec),
        super::PhysicalFamily::TranscriptViewEntries => reserve!(TranscriptEntriesCodec),
        super::PhysicalFamily::StableItemProjections => reserve!(StableItemProjectionsCodec),
        super::PhysicalFamily::ItemProjections => reserve!(ItemProjectionsCodec),
        super::PhysicalFamily::ProjectionResources => reserve!(ProjectionResourcesCodec),
        super::PhysicalFamily::BindingHeads => reserve!(BindingHeadsCodec),
        super::PhysicalFamily::CasThread => reserve!(CasThreadIndexCodec),
        super::PhysicalFamily::CasThreadBinding => reserve!(CasThreadBindingIndexCodec),
        super::PhysicalFamily::CasTurn => reserve!(CasTurnIndexCodec),
    }
    Ok(())
}

impl SyndicStorage {
    /// Seals one bounded typed fixture batch against an exact expected domain revision.
    #[must_use]
    pub fn fixture_contribution(
        &self,
        expected_revision: DomainRevision,
        batch: FixtureBatch,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, batch)
    }

    /// Reads one immutable CAS-thread binding membership for fault-cut assertions.
    pub fn fixture_cas_thread_binding_membership(
        &self,
        store: &beryl_home_store::HomeStore,
        cas_thread: beryl_model::CasThreadId,
        revision: beryl_model::BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CasThreadBindingIndexRecord>, SyndicReadError> {
        self.point::<CasThreadBindingIndexFamily>(
            store,
            CasThreadBindingKey::Record(cas_thread, revision),
            limit,
        )
    }

    /// Counts a bounded physical slice of activity entries, including logically retired rows.
    pub fn fixture_activity_query_entry_count(
        &self,
        store: &beryl_home_store::HomeStore,
        thread: beryl_model::SyndicThreadId,
        work_period: ActivityWorkPeriod,
        limits: CursorReadLimits,
    ) -> Result<(usize, bool), SyndicReadError> {
        let page = store.read_cursor::<SyndicDomain, ActivityQueryEntriesCodec>(
            &self.handle,
            &CursorRange::closed(
                ActivityQueryEntryKey::first_for_period(thread, work_period),
                ActivityQueryEntryKey::last_for_period(thread, work_period),
            ),
            CursorDirection::Forward,
            limits,
        )?;
        Ok((page.records().len(), page.has_more()))
    }
}
