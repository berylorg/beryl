use beryl_home_store::{
    AdmittedSidecar, CommandBuildError, CursorDirection, CursorRange, CursorReadLimits,
    DomainHandle, DomainReconciliation, DomainRegistrationError, DomainSchemaVersion, HomeCommand,
    HomeStore, KeyspaceSchemaVersion, MutationContribution, PointReadLimit, ReadError,
    ReconciliationReader, RecordFamily, SidecarAddress, SidecarByteLimit, SidecarDigest,
    SidecarError, SidecarNamespace, SidecarVerifier, StorageDomain, ValidationContribution,
    VerifiedSidecar,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, DomainRevision, ImageLabelOrdinal, SealedAssetReferenceSetProof,
    SyndicDraftMarkerId,
};

use crate::StatePage;

mod codec;
mod completion;
mod digest;
mod error;
mod footprint;
mod model;
mod mutation;
mod read;
#[cfg(feature = "test-faults")]
mod test_support;
mod validate;

use codec::{
    AssetMetadataCodec, AssetOwnerHeadCodec, AssetReferenceCompletionEvidenceCodec,
    AssetReferenceEntryCodec, AssetReferenceLabelFirstCodec, AssetReferenceManifestCodec,
    AssetReferenceMarkerCodec,
};
pub use error::{
    AssetAdmissionError, AssetMutationError, AssetOwnerHeadUpdateError,
    AssetOwnerHeadValidationError, AssetReadError, AssetReferencePageError, AssetValidationError,
    AssetValueError,
};
pub use footprint::{
    accepted_input_to_submitted_item_owner_transfer_max_footprint,
    draft_to_submitted_item_owner_transfer_max_footprint,
};
#[cfg(feature = "test-faults")]
pub use model::AssetReferenceSetManifestCorruption;
pub use model::{
    AssetDimensions, AssetLabelDisposition, AssetMediaType, AssetMetadataRecord, AssetOwner,
    AssetOwnerHeadExpectation, AssetOwnerHeadRecord, AssetReferenceEntryRecord,
    AssetReferenceOrdinal, AssetReferenceSetBuildProof, AssetReferenceSetCompletion,
    AssetReferenceSetLifecycle, AssetReferenceSetManifest, AssetReferenceSetStagingAuthority,
    AssetSidecarState,
};
use model::{
    AssetEntryKey, AssetLabelFirstKey, AssetLabelFirstRecord, AssetMarkerKey,
    AssetReferenceSetCompletionEvidence,
};
pub use mutation::{
    AppendAssetReferencePage, AssetOwnerHeadAssertion, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, BeginAssetReferenceSet, PublishAssetMetadata, SealAssetReferenceSet,
    UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};

const ASSET_NAMESPACE: &str = "images";
const ASSET_METADATA_LIMIT: usize = 256;
const ASSET_MANIFEST_LIMIT: usize = 256;
const ASSET_COMPLETION_EVIDENCE_LIMIT: usize = 128;
const ASSET_ENTRY_LIMIT: usize = 256;
const ASSET_INDEX_LIMIT: usize = 128;
const ASSET_HEAD_LIMIT: usize = 512;
const MAX_MEDIA_TYPE_BYTES: usize = 127;

/// Maximum number of marker entries carried by one physically bounded append command.
pub const ASSET_REFERENCE_PAGE_MAX_ENTRIES: usize = 64;

/// Maximum cumulative stored key and value bytes returned by one public entry page.
pub const ASSET_REFERENCE_PAGE_MAX_STORED_BYTES: usize = 65_536;

/// Maximum number of compact owner-head changes in one atomic asset contribution.
pub const ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES: usize = 4;

const ASSET_FAMILIES: &[RecordFamily<AssetDomain>] = &[
    RecordFamily::new::<AssetMetadataCodec>(KeyspaceSchemaVersion::new(3)),
    RecordFamily::new::<AssetReferenceManifestCodec>(KeyspaceSchemaVersion::new(3)),
    RecordFamily::new::<AssetReferenceCompletionEvidenceCodec>(KeyspaceSchemaVersion::new(3)),
    RecordFamily::new::<AssetReferenceEntryCodec>(KeyspaceSchemaVersion::new(3)),
    RecordFamily::new::<AssetReferenceMarkerCodec>(KeyspaceSchemaVersion::new(3)),
    RecordFamily::new::<AssetReferenceLabelFirstCodec>(KeyspaceSchemaVersion::new(3)),
    RecordFamily::new::<AssetOwnerHeadCodec>(KeyspaceSchemaVersion::new(3)),
];

pub(crate) struct AssetDomain;

impl StorageDomain for AssetDomain {
    const NAME: &'static str = "beryl-assets";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(3);
    const FAMILIES: &'static [RecordFamily<Self>] = ASSET_FAMILIES;
    type ValidationError = AssetValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }

    fn validate_reopen(
        reader: &beryl_home_store::DomainReader<'_, Self>,
        sidecars: &SidecarVerifier<'_>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate_reopen(reader, sidecars)
    }

    fn reconcile(
        reader: &ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, Self::ValidationError> {
        let mut classification = crate::reconciliation::ReconciliationClassification::new();
        crate::reconciliation::classify_records::<Self, AssetMetadataCodec>(
            reader,
            &mut classification,
        )?;
        crate::reconciliation::classify_records::<Self, AssetReferenceManifestCodec>(
            reader,
            &mut classification,
        )?;
        crate::reconciliation::classify_records::<Self, AssetReferenceCompletionEvidenceCodec>(
            reader,
            &mut classification,
        )?;
        crate::reconciliation::classify_records::<Self, AssetReferenceEntryCodec>(
            reader,
            &mut classification,
        )?;
        crate::reconciliation::classify_records::<Self, AssetReferenceMarkerCodec>(
            reader,
            &mut classification,
        )?;
        crate::reconciliation::classify_records::<Self, AssetReferenceLabelFirstCodec>(
            reader,
            &mut classification,
        )?;
        crate::reconciliation::classify_records::<Self, AssetOwnerHeadCodec>(
            reader,
            &mut classification,
        )?;
        Ok(classification.finish())
    }
}

/// Opaque typed access to durable asset metadata, reference-set staging, and owner heads.
#[derive(Clone, Copy)]
pub struct AssetState {
    handle: DomainHandle<AssetDomain>,
}

impl AssetState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<AssetDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn register_with_schema_validation(
        store: &mut HomeStore,
    ) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain_with_schema_validation::<AssetDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<AssetDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire_candidate(
        candidate: &beryl_home_store::HomeRecoveryCandidate,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        candidate
            .domain_handle::<AssetDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    pub fn committed_revision(
        &self,
        store: &HomeStore,
        receipt: &beryl_home_store::CommitReceipt,
    ) -> Result<Option<DomainRevision>, beryl_home_store::CommitReceiptError> {
        store.receipt_domain_revision(receipt, self.handle)
    }

    pub fn metadata(
        &self,
        store: &HomeStore,
        asset_id: AssetId,
    ) -> Result<Option<AssetMetadataRecord>, ReadError> {
        store.read_point::<AssetDomain, AssetMetadataCodec>(
            self.handle,
            &asset_id,
            metadata_point_limit(),
        )
    }

    /// Reads current state only for the unpublished build named by typed staging authority.
    pub fn staged_reference_set_manifest(
        &self,
        store: &HomeStore,
        authority: AssetReferenceSetStagingAuthority,
    ) -> Result<AssetReferenceSetManifest, AssetReadError> {
        read::require_building_manifest(self, store, authority)
    }

    /// Reads a sealed manifest only after rechecking its complete opaque proof.
    pub fn sealed_reference_set_manifest(
        &self,
        store: &HomeStore,
        proof: SealedAssetReferenceSetProof,
    ) -> Result<AssetReferenceSetManifest, AssetReadError> {
        read::require_sealed_manifest(self, store, proof)
    }

    pub fn reference_set_entries(
        &self,
        store: &HomeStore,
        proof: SealedAssetReferenceSetProof,
        after: Option<AssetReferenceOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<AssetReferenceEntryRecord>, AssetReadError> {
        read::require_sealed_manifest(self, store, proof)?;
        let set_id = proof.set_id();
        let limits = CursorReadLimits::new(
            limits.max_items().min(ASSET_REFERENCE_PAGE_MAX_ENTRIES),
            limits
                .max_bytes()
                .min(ASSET_REFERENCE_PAGE_MAX_STORED_BYTES),
        )
        .expect("caller limits and package clamps are nonzero");
        let start = AssetEntryKey {
            set_id,
            ordinal: after.unwrap_or(AssetReferenceOrdinal::FIRST),
        };
        let end = AssetEntryKey {
            set_id,
            ordinal: AssetReferenceOrdinal::new(u64::MAX).expect("maximum ordinal is nonzero"),
        };
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_cursor::<AssetDomain, AssetReferenceEntryCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        Ok(StatePage {
            stored_bytes: page.stored_bytes(),
            decoded_bytes: page.decoded_bytes(),
            has_more: page.has_more(),
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
        })
    }

    pub fn marker_reference(
        &self,
        store: &HomeStore,
        proof: SealedAssetReferenceSetProof,
        marker_id: SyndicDraftMarkerId,
    ) -> Result<Option<AssetReferenceEntryRecord>, AssetReadError> {
        read::require_sealed_manifest(self, store, proof)?;
        let set_id = proof.set_id();
        let key = AssetMarkerKey { set_id, marker_id };
        let Some(ordinal) = store.read_point::<AssetDomain, AssetReferenceMarkerCodec>(
            self.handle,
            &key,
            index_point_limit(),
        )?
        else {
            return Ok(None);
        };
        Ok(store.read_point::<AssetDomain, AssetReferenceEntryCodec>(
            self.handle,
            &AssetEntryKey { set_id, ordinal },
            entry_point_limit(),
        )?)
    }

    /// Resolves the first exact asset reference for one label through bounded point reads.
    pub fn label_first_reference(
        &self,
        store: &HomeStore,
        proof: SealedAssetReferenceSetProof,
        label: ImageLabelOrdinal,
    ) -> Result<Option<AssetReferenceEntryRecord>, AssetReadError> {
        read::require_sealed_manifest(self, store, proof)?;
        let set_id = proof.set_id();
        let key = AssetLabelFirstKey { set_id, label };
        let Some(first) = store.read_point::<AssetDomain, AssetReferenceLabelFirstCodec>(
            self.handle,
            &key,
            index_point_limit(),
        )?
        else {
            return Ok(None);
        };
        Ok(store.read_point::<AssetDomain, AssetReferenceEntryCodec>(
            self.handle,
            &AssetEntryKey {
                set_id,
                ordinal: first.first_ordinal,
            },
            entry_point_limit(),
        )?)
    }

    pub fn owner_head(
        &self,
        store: &HomeStore,
        owner: AssetOwner,
    ) -> Result<Option<AssetOwnerHeadRecord>, ReadError> {
        store.read_point::<AssetDomain, AssetOwnerHeadCodec>(
            self.handle,
            &owner,
            head_point_limit(),
        )
    }

    /// Verifies exactly the length carried by `asset_id` and retains the file-backed authority.
    pub fn verify_sidecar(
        &self,
        store: &HomeStore,
        asset_id: AssetId,
    ) -> Result<VerifiedSidecar, SidecarError> {
        store.verify_sidecar(
            &asset_sidecar_address(asset_id),
            SidecarByteLimit::new(asset_id.length()),
        )
    }

    /// Publishes immutable metadata independently from any owner head.
    pub fn publish_metadata(
        &self,
        expected_revision: DomainRevision,
        sidecar: AdmittedSidecar,
        command: PublishAssetMetadata,
    ) -> Result<AssetMetadataContribution, AssetAdmissionError> {
        ensure_sidecar_matches(sidecar.address(), command.asset_id())?;
        let creation_revision = expected_revision
            .checked_next()
            .map_err(|_| AssetAdmissionError::RevisionExhausted)?;
        if command.creation_revision() != creation_revision {
            return Err(AssetAdmissionError::CreationRevisionMismatch {
                expected: creation_revision,
                actual: command.creation_revision(),
            });
        }
        Ok(AssetMetadataContribution {
            sidecar,
            contribution: self.handle.contribution(expected_revision, command),
        })
    }

    #[must_use]
    pub fn begin_reference_set(
        &self,
        expected_revision: DomainRevision,
        command: BeginAssetReferenceSet,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn append_reference_page(
        &self,
        expected_revision: DomainRevision,
        command: AppendAssetReferencePage,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn seal_reference_set(
        &self,
        expected_revision: DomainRevision,
        command: SealAssetReferenceSet,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn update_owner_heads(
        &self,
        expected_revision: DomainRevision,
        command: UpdateAssetOwnerHeads,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    /// Seals an exact owner-head assertion for a heterogeneous mutating home command.
    #[must_use]
    pub fn validate_owner_heads(
        &self,
        expected_revision: DomainRevision,
        validator: ValidateAssetOwnerHeads,
    ) -> ValidationContribution {
        self.handle.validation(expected_revision, validator)
    }

    /// Builds a deliberately incompatible owner-head proof for reopen validation tests.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    #[must_use]
    pub fn corrupt_owner_head_proof_for_test(
        &self,
        expected_revision: DomainRevision,
        owner: AssetOwner,
        expected: AssetOwnerHeadExpectation,
        replacement: SealedAssetReferenceSetProof,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_revision,
            test_support::CorruptOwnerHeadProof {
                owner,
                expected,
                replacement,
            },
        )
    }

    #[cfg(feature = "test-faults")]
    #[must_use]
    pub fn corrupt_reference_set_manifest_for_test(
        &self,
        expected_revision: DomainRevision,
        set_id: AssetReferenceSetId,
        corruption: AssetReferenceSetManifestCorruption,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_revision,
            test_support::CorruptReferenceSetManifest { set_id, corruption },
        )
    }

    #[cfg(feature = "test-faults")]
    #[must_use]
    pub fn remove_reference_set_completion_evidence_for_test(
        &self,
        expected_revision: DomainRevision,
        set_id: AssetReferenceSetId,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_revision,
            test_support::RemoveReferenceSetCompletionEvidence { set_id },
        )
    }
}

/// Metadata contribution that cannot be separated from its admitted sidecar token.
pub struct AssetMetadataContribution {
    sidecar: AdmittedSidecar,
    contribution: MutationContribution,
}

impl AssetMetadataContribution {
    pub fn add_to(self, command: &mut HomeCommand) -> Result<(), CommandBuildError> {
        command.require_sidecar(self.sidecar)?;
        command.add(self.contribution)?;
        Ok(())
    }
}

fn ensure_sidecar_matches(
    address: &SidecarAddress,
    asset_id: AssetId,
) -> Result<(), AssetAdmissionError> {
    if address.namespace().as_str() != ASSET_NAMESPACE {
        return Err(AssetAdmissionError::WrongNamespace);
    }
    if address.digest().as_bytes() != asset_id.digest()
        || address.length() != asset_id.length().get()
    {
        return Err(AssetAdmissionError::IdentityMismatch);
    }
    Ok(())
}

fn asset_sidecar_address(asset_id: AssetId) -> SidecarAddress {
    SidecarAddress::new(
        SidecarNamespace::new(ASSET_NAMESPACE).expect("asset sidecar namespace is valid"),
        SidecarDigest::from_bytes(asset_id.digest()),
        asset_id.length().get(),
    )
}

fn metadata_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_METADATA_LIMIT + 4).expect("metadata point bound is nonzero")
}

fn manifest_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_MANIFEST_LIMIT + 4).expect("manifest point bound is nonzero")
}

fn entry_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_ENTRY_LIMIT + 4).expect("entry point bound is nonzero")
}

fn index_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_INDEX_LIMIT + 4).expect("index point bound is nonzero")
}

fn head_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_HEAD_LIMIT + 4).expect("head point bound is nonzero")
}
