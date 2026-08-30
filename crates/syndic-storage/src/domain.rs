use std::convert::Infallible;

use beryl_home_store::{
    CommitReceipt, CommitReceiptError, DomainHandle, DomainHandleError, DomainReconciliation,
    DomainRegistrationError, DomainSchemaVersion, HomeRecoveryCandidate, HomeStore,
    KeyspaceSchemaVersion, ReadError, ReconciliationReader, RecordCodec, RecordFamily,
    StorageDomain,
};
use beryl_model::DomainRevision;

use crate::{codec::*, draft_piece::*, error::SyndicValidationError};

const V7_FAMILIES: &[RecordFamily<SyndicDomain>] = &[
    RecordFamily::new::<ThreadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ImageLabelAuthorityHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftImageLabelProtectionHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadExecutionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadAttributesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadUsageCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ThreadCatalogSummariesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceRootsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceNodesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceLeavesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMarkerIdentityIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMarkerOrderCommitmentsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMarkerSealsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceBuildsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceBuildFragmentsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceBuildProgressCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftPieceSettlementsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftEditorCandidateSessionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMutationStagingHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMutationStagingPagesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMutationStagingProgressCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftEditHistoryFrontiersCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftEditHistoryTransitionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftHistoricalRootAdoptionsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftComposerBuildsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftComposerMaterializationsCodec>(KeyspaceSchemaVersion::new(1)),
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
    RecordFamily::new::<DraftMarkerAdmissionCapacityCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMarkerAdmissionHeadsCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMarkerAdmissionNodesCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DraftMarkerAdmissionReceiptsCodec>(KeyspaceSchemaVersion::new(1)),
];
const _: [(); 86] = [(); V7_FAMILIES.len()];

#[cfg(feature = "test-faults")]
pub(crate) fn v7_family_names() -> impl Iterator<Item = &'static str> {
    V7_FAMILIES.iter().map(RecordFamily::name)
}

pub(crate) struct SyndicDomain;

impl StorageDomain for SyndicDomain {
    const NAME: &'static str = "syndic";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(7);
    const FAMILIES: &'static [RecordFamily<Self>] = V7_FAMILIES;
    type ValidationError = SyndicValidationError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment(
        _reader: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<Self::RuntimeAttachment, Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        crate::validation::validate(reader)
    }

    fn reconcile(
        reader: &ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, Self::ValidationError> {
        let mut sides = ReconciliationSides::default();
        macro_rules! classify {
            ($codec:ty) => {
                classify_records::<$codec>(reader, &mut sides)?;
            };
        }
        classify!(ThreadsCodec);
        classify!(ImageLabelAuthorityHeadsCodec);
        classify!(DraftImageLabelProtectionHeadsCodec);
        classify!(ThreadExecutionsCodec);
        classify!(ThreadAttributesCodec);
        classify!(ThreadUsageCodec);
        classify!(ThreadCatalogSummariesCodec);
        classify!(DraftsCodec);
        classify!(DraftPieceRootsCodec);
        classify!(DraftPieceNodesCodec);
        classify!(DraftPieceLeavesCodec);
        classify!(DraftMarkerIdentityIndexCodec);
        classify!(DraftMarkerOrderCommitmentsCodec);
        classify!(DraftMarkerSealsCodec);
        classify!(DraftPieceBuildsCodec);
        classify!(DraftPieceBuildFragmentsCodec);
        classify!(DraftPieceBuildProgressCodec);
        classify!(DraftPieceSettlementsCodec);
        classify!(DraftEditorCandidateSessionsCodec);
        classify!(DraftMutationStagingHeadsCodec);
        classify!(DraftMutationStagingPagesCodec);
        classify!(DraftMutationStagingProgressCodec);
        classify!(DraftEditHistoryFrontiersCodec);
        classify!(DraftEditHistoryTransitionsCodec);
        classify!(DraftHistoricalRootAdoptionsCodec);
        classify!(DraftComposerBuildsCodec);
        classify!(DraftComposerMaterializationsCodec);
        classify!(ContentManifestsCodec);
        classify!(ContentChunksCodec);
        classify!(ContentByteSpansCodec);
        classify!(ContentTextSpansCodec);
        classify!(ProviderNarrativeSpansCodec);
        classify!(ContentPiecesCodec);
        classify!(ContextEnvelopesCodec);
        classify!(TurnsCodec);
        classify!(TurnStatesCodec);
        classify!(InputGatesCodec);
        classify!(AcceptedInputsCodec);
        classify!(StopOperationsCodec);
        classify!(CompactionOperationsCodec);
        classify!(CompactionSettlementReceiptsCodec);
        classify!(AcceptedRouteGenerationHeadsCodec);
        classify!(AcceptedRouteLeavesCodec);
        classify!(SourceEventsCodec);
        classify!(ProviderObservationBuildsCodec);
        classify!(ProviderItemBuildsCodec);
        classify!(CanonicalItemsCodec);
        classify!(ActivityQueryHeadsCodec);
        classify!(ItemProjectionHeadsCodec);
        classify!(ItemProjectionSetsCodec);
        classify!(ItemProjectionBuildsCodec);
        classify!(TranscriptHeadsCodec);
        classify!(TranscriptBuildsCodec);
        classify!(ProjectionsCodec);
        classify!(ResourcesCodec);
        classify!(HistorySummariesCodec);
        classify!(BindingsCodec);
        classify!(ExecutionSnapshotsCodec);
        classify!(ActiveCasTurnsCodec);
        classify!(DraftByThreadCodec);
        classify!(ThreadParentCodec);
        classify!(ImageLabelOriginSpansCodec);
        classify!(TurnChildrenCodec);
        classify!(AcceptedOrderCodec);
        classify!(AcceptedRouteGenerationsCodec);
        classify!(AcceptedReadySourcesCodec);
        classify!(AcceptedNextSourcesCodec);
        classify!(TurnItemsCodec);
        classify!(ActivityQueryEntriesCodec);
        classify!(ActivityQuerySourcesCodec);
        classify!(ItemSourceEventsCodec);
        classify!(CasItemIndexCodec);
        classify!(TranscriptPathTurnsCodec);
        classify!(TranscriptEntriesCodec);
        classify!(StableItemProjectionsCodec);
        classify!(ItemProjectionsCodec);
        classify!(ProjectionResourcesCodec);
        classify!(BindingHeadsCodec);
        classify!(CasThreadIndexCodec);
        classify!(CasThreadBindingIndexCodec);
        classify!(CasTurnIndexCodec);
        classify!(ProviderObservationChunksCodec);
        classify!(DraftMarkerAdmissionCapacityCodec);
        classify!(DraftMarkerAdmissionHeadsCodec);
        classify!(DraftMarkerAdmissionNodesCodec);
        classify!(DraftMarkerAdmissionReceiptsCodec);
        Ok(sides.finish())
    }
}

struct ReconciliationSides {
    saw_record: bool,
    exact_old: bool,
    exact_new: bool,
}

impl Default for ReconciliationSides {
    fn default() -> Self {
        Self {
            saw_record: false,
            exact_old: true,
            exact_new: true,
        }
    }
}

impl ReconciliationSides {
    fn finish(&self) -> DomainReconciliation {
        match (self.saw_record, self.exact_old, self.exact_new) {
            (true, true, false) => DomainReconciliation::ExactOld,
            (true, false, true) => DomainReconciliation::ExactNew,
            _ => DomainReconciliation::Collision,
        }
    }
}

fn classify_records<R>(
    reader: &ReconciliationReader<'_, SyndicDomain>,
    sides: &mut ReconciliationSides,
) -> Result<(), SyndicValidationError>
where
    R: RecordCodec<SyndicDomain>,
    R::Value: PartialEq,
{
    for record in reader.records::<R>()? {
        sides.saw_record = true;
        sides.exact_old &= record.current() == record.old();
        sides.exact_new &= record.current() == record.new();
    }
    Ok(())
}

#[derive(Clone)]
pub struct SyndicStorage {
    pub(crate) handle: DomainHandle<SyndicDomain>,
}

impl SyndicStorage {
    pub fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<SyndicDomain>()
            .map(|handle| Self { handle })
    }

    pub fn register_with_schema_validation(
        store: &mut HomeStore,
    ) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain_with_schema_validation::<SyndicDomain>()
            .map(|handle| Self { handle })
    }

    /// Reacquires this exact typed domain after successful same-home recovery without a record scan.
    pub fn reacquire(store: &HomeStore) -> Result<Self, DomainHandleError> {
        store
            .domain_handle::<SyndicDomain>()
            .map(|handle| Self { handle })
    }

    /// Reacquires this exact typed domain from an unpublished same-home recovery candidate.
    ///
    /// This is declaration-, family-, exact-type-, and generation-bound only; it does not scan
    /// persisted application records or open ordinary store admission.
    pub fn reacquire_candidate(
        candidate: &HomeRecoveryCandidate,
    ) -> Result<Self, DomainHandleError> {
        candidate
            .domain_handle::<SyndicDomain>()
            .map(|handle| Self { handle })
    }

    /// Returns the exact current Syndic domain revision.
    pub fn revision(&self, store: &HomeStore) -> Result<DomainRevision, ReadError> {
        store.domain_revision(&self.handle)
    }

    /// Returns this domain's revision from a still-current successful command receipt.
    pub fn committed_revision(
        &self,
        store: &HomeStore,
        receipt: &CommitReceipt,
    ) -> Result<Option<DomainRevision>, CommitReceiptError> {
        store.receipt_domain_revision(receipt, &self.handle)
    }
}
