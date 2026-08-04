use beryl_home_store::{
    CommitReceipt, CommitReceiptError, DomainHandle, DomainHandleError, DomainRegistrationError,
    DomainSchemaVersion, HomeStore, KeyspaceSchemaVersion, ReadError, RecordFamily, StorageDomain,
};
use beryl_model::DomainRevision;

use crate::{codec::*, error::SyndicValidationError};

const V5_FAMILIES: &[RecordFamily<SyndicDomain>] = &[
    RecordFamily::new::<ThreadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadExecutionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadAttributesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadUsageCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadCatalogSummariesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ContentManifestsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ContentChunksCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ContentByteSpansCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ContentTextSpansCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ProviderNarrativeSpansCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ContentPiecesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ContextEnvelopesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TurnsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TurnStatesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<InputGatesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedInputsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<StopOperationsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CompactionOperationsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CompactionSettlementReceiptsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedRouteGenerationHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedRouteLeavesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<SourceEventsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ProviderObservationBuildsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ProviderItemBuildsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CanonicalItemsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ActivityQueryHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ItemProjectionHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ItemProjectionSetsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ItemProjectionBuildsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TranscriptHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TranscriptBuildsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ProjectionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ResourcesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<HistorySummariesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<BindingsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ExecutionSnapshotsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ActiveCasTurnsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftByThreadCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadParentCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ImageLabelOriginSpansCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TurnChildrenCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedOrderCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedRouteGenerationsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedReadySourcesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<AcceptedNextSourcesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TurnItemsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ActivityQueryEntriesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ActivityQuerySourcesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ItemSourceEventsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CasItemIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TranscriptPathTurnsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<TranscriptEntriesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<StableItemProjectionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ItemProjectionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ProjectionResourcesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<BindingHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CasThreadIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CasThreadBindingIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<CasTurnIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ProviderObservationChunksCodec>(KeyspaceSchemaVersion::new(1)),
];
const _: [(); 61] = [(); V5_FAMILIES.len()];

#[cfg(feature = "test-faults")]
pub(crate) fn v5_family_names() -> impl Iterator<Item = &'static str> {
    V5_FAMILIES.iter().map(RecordFamily::name)
}

pub(crate) struct SyndicDomain;

impl StorageDomain for SyndicDomain {
    const NAME: &'static str = "syndic";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(5);
    const FAMILIES: &'static [RecordFamily<Self>] = V5_FAMILIES;
    type ValidationError = SyndicValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        crate::validation::validate(reader)
    }
}

/// Opaque typed access to the permanent Syndic V5 domain in one Beryl home.
#[derive(Clone, Copy)]
pub struct SyndicStorage {
    pub(crate) handle: DomainHandle<SyndicDomain>,
}

impl SyndicStorage {
    /// Registers the exact V5 domain and validates every persisted family before publication.
    pub fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<SyndicDomain>()
            .map(|handle| Self { handle })
    }

    /// Reacquires this exact typed domain after successful same-home recovery.
    pub fn reacquire(store: &HomeStore) -> Result<Self, DomainHandleError> {
        store
            .domain_handle::<SyndicDomain>()
            .map(|handle| Self { handle })
    }

    /// Returns the exact current Syndic domain revision.
    pub fn revision(&self, store: &HomeStore) -> Result<DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    /// Returns this domain's revision from a still-current successful command receipt.
    pub fn committed_revision(
        &self,
        store: &HomeStore,
        receipt: &CommitReceipt,
    ) -> Result<Option<DomainRevision>, CommitReceiptError> {
        store.receipt_domain_revision(receipt, self.handle)
    }
}
