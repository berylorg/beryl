use super::*;
use crate::{codec::*, draft_piece::*};
use beryl_home_store::{
    DomainReader, DomainRegistrationError, DomainRegistrationReader, DomainSchemaVersion,
    HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, RecordCodec,
    RecordFamily, StorageDomain,
};
use std::convert::Infallible;

struct DifferentDeclaration;

struct OmittedCompanionsCodec<R>(std::marker::PhantomData<R>);

impl<R: RecordCodec<SyndicDomain>> RecordCodec<DifferentDeclaration> for OmittedCompanionsCodec<R> {
    type Key = R::Key;
    type Value = R::Value;
    type Error = R::Error;
    const FAMILY: &'static str = R::FAMILY;
    const VERSION: beryl_home_store::RecordVersion = R::VERSION;
    const MAX_KEY_BYTES: usize = R::MAX_KEY_BYTES;
    const MAX_VALUE_BYTES: usize = R::MAX_VALUE_BYTES;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        R::encode_key(key)
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, Self::Error> {
        R::decode_key(bytes)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        R::encode_value(value)
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, Self::Error> {
        R::decode_value(bytes)
    }
}

impl StorageDomain for DifferentDeclaration {
    const NAME: &'static str = "syndic";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(7);
    const FAMILIES: &'static [RecordFamily<Self>] = &[
        RecordFamily::new::<OmittedCompanionsCodec<ThreadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ImageLabelAuthorityHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftImageLabelProtectionHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ThreadExecutionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ThreadAttributesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ThreadUsageCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ThreadCatalogSummariesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceRootsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceNodesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceLeavesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerIdentityIndexCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerOrderCommitmentsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerSealsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceBuildsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceBuildFragmentsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceBuildProgressCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceSettlementsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftEditorCandidateSessionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMutationStagingHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMutationStagingPagesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMutationStagingProgressCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftEditHistoryFrontiersCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftEditHistoryTransitionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftHistoricalRootAdoptionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftComposerBuildsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftComposerMaterializationsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ContentManifestsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ContentChunksCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ContentByteSpansCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ContentTextSpansCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ProviderNarrativeSpansCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ContentPiecesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ContextEnvelopesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TurnsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TurnStatesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<InputGatesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedInputsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<StopOperationsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CompactionOperationsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CompactionSettlementReceiptsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedRouteGenerationHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedRouteLeavesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<SourceEventsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ProviderObservationBuildsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ProviderItemBuildsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CanonicalItemsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ActivityQueryHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ItemProjectionHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ItemProjectionSetsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ItemProjectionBuildsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TranscriptHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TranscriptBuildsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ProjectionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ResourcesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<HistorySummariesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<BindingsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ExecutionSnapshotsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ActiveCasTurnsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftByThreadCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ThreadParentCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ImageLabelOriginSpansCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TurnChildrenCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedOrderCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedRouteGenerationsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedReadySourcesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<AcceptedNextSourcesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TurnItemsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ActivityQueryEntriesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ActivityQuerySourcesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ItemSourceEventsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CasItemIndexCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TranscriptPathTurnsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<TranscriptEntriesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<StableItemProjectionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ItemProjectionsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ProjectionResourcesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<BindingHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CasThreadIndexCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CasThreadBindingIndexCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<CasTurnIndexCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<ProviderObservationChunksCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerAdmissionCapacityCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerAdmissionHeadsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerAdmissionNodesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftMarkerAdmissionReceiptsCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DraftPieceBuildMappingCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<NonIdleGateSourcesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
        RecordFamily::new::<OmittedCompanionsCodec<DiscussionHandoffGatesCodec>>(
            beryl_home_store::KeyspaceSchemaVersion::new(1),
        ),
    ];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment(_: &DomainRegistrationReader<'_, Self>) -> Result<(), Infallible> {
        Ok(())
    }
    fn validate(_: &DomainReader<'_, Self>) -> Result<(), Infallible> {
        Ok(())
    }
}

#[test]
fn exact_companion_inventory_is_appended_and_matching_empty_families_reopen() {
    let names: Vec<_> = crate::domain::v7_family_names().collect();
    assert_eq!(names.len(), 101);
    assert_eq!(
        &names[89..99],
        &[
            "resource-structure-builds",
            "resource-structure-progress",
            "resource-structure-roots",
            "resource-structure-heads",
            "resource-structure-pages",
            "resource-code-lines",
            "resource-table-rows",
            "resource-table-cells",
            "resource-table-columns",
            "resource-source-anchors",
        ]
    );
    assert_eq!(
        names
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        101
    );
    let home = TestHome::new();
    let faults = FaultController::new();
    let (store, handle) = open(&home, &faults);
    assert!(read::<BuildsCodec>(&store, &handle, &key()).is_none());
    assert!(read::<HeadsCodec>(&store, &handle, &key().resource).is_none());
    store.close().unwrap();
    let (store, handle) = open(&home, &faults);
    assert!(read::<RootsCodec>(&store, &handle, &key()).is_none());
    assert!(
        read::<PagesCodec>(
            &store,
            &handle,
            &SequenceKey {
                build: key(),
                ordinal: 1
            }
        )
        .is_none()
    );
    store.close().unwrap();
}

#[test]
fn declaration_mismatch_rejects_before_access_and_preserves_exact_home() {
    let home = TestHome::new();
    let faults = FaultController::new();
    let (store, _) = open(&home, &faults);
    store.close().unwrap();
    {
        let mut candidate =
            HomeOpenCandidate::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT))
                .unwrap();
        let result = candidate.register_domain::<DifferentDeclaration>();
        assert!(
            matches!(
                result,
                Err(DomainRegistrationError::IncompatibleKeyspaces { domain: "syndic" })
            ),
            "{result:?}"
        );
        assert!(
            candidate
                .prepare_publication(
                    HomeDomainRequirements::new()
                        .with_domain::<DifferentDeclaration>()
                        .unwrap()
                )
                .is_err()
        );
    }
    let (store, _) = open(&home, &faults);
    store.close().unwrap();
}

#[test]
fn routine_open_does_not_scan_unselected_companion_pages() {
    let home = TestHome::new();
    let faults = FaultController::new();
    let (store, handle) = open(&home, &faults);
    let page = SequenceKey {
        build: key(),
        ordinal: 1,
    };
    let encoded_key = <PagesCodec as RecordCodec<SyndicDomain>>::encode_key(&page).unwrap();
    store
        .inject_persisted_corrupt_record::<SyndicDomain, PagesCodec>(&handle, &encoded_key, &[0])
        .unwrap();
    store.close().unwrap();
    let (store, handle) = open(&home, &faults);
    assert!(
        store
            .read_point::<SyndicDomain, PagesCodec>(
                &handle,
                &page,
                beryl_home_store::PointReadLimit::new(65_664).unwrap()
            )
            .is_err()
    );
    store.close().unwrap();
}

#[test]
fn home_without_companion_declaration_refuses_larger_declaration_without_mutation() {
    let home = TestHome::new();
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    candidate.register_domain::<DifferentDeclaration>().unwrap();
    candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<DifferentDeclaration>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap()
        .close()
        .unwrap();
    {
        let mut candidate =
            HomeOpenCandidate::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT))
                .unwrap();
        assert!(matches!(
            candidate.register_domain::<SyndicDomain>(),
            Err(DomainRegistrationError::IncompatibleKeyspaces { domain: "syndic" })
        ));
        assert!(
            candidate
                .prepare_publication(crate::SyndicStorage::required_domains().unwrap())
                .is_err()
        );
    }
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    candidate.register_domain::<DifferentDeclaration>().unwrap();
    candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<DifferentDeclaration>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap()
        .close()
        .unwrap();
}
