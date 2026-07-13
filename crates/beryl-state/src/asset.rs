use std::num::NonZeroU64;

use beryl_home_store::{
    AdmittedSidecar, CommandBuildError, CursorDirection, CursorRange, CursorReadLimits,
    DomainHandle, DomainRegistrationError, DomainSchemaVersion, HomeCommand, HomeStore,
    KeyspaceFamily, KeyspaceSchemaVersion, MutationContribution, PointReadLimit, ReadError,
    SidecarAddress, SidecarVerifier, StorageDomain,
};
use beryl_model::{
    AssetId, DomainRevision, SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId,
    SyndicItemId, SyndicProjectionId, SyndicQueuedInputId, SyndicRetryRecordId,
};

use crate::{RecordRevision, StatePage, UnixMillis};

mod codec;
mod error;
mod mutation;
#[cfg(test)]
mod tests;
mod validate;

use codec::{AssetMetadataCodec, AssetReferenceCodec, AssetReferenceIndexCodec};
pub use error::{AssetAdmissionError, AssetMutationError, AssetValidationError, AssetValueError};
pub use mutation::{AddAssetReference, CreateAssetWithReference, RemoveAssetReference};

const ASSET_NAMESPACE: &str = "images";
const ASSET_METADATA_LIMIT: usize = 1_024;
const ASSET_REFERENCE_LIMIT: usize = 256;
const MAX_MEDIA_TYPE_BYTES: usize = 127;
const MAX_ASSET_BYTES: u64 = 512 * 1_024 * 1_024;

const ASSET_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("metadata", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("references", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("references_by_asset", KeyspaceSchemaVersion::new(1)),
];

pub(crate) struct AssetDomain;

impl StorageDomain for AssetDomain {
    const NAME: &'static str = "beryl-assets";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = ASSET_FAMILIES;
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
}

/// Validated media type retained with durable image metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMediaType(Box<str>);

impl AssetMediaType {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AssetValueError> {
        let value = value.as_ref();
        let valid = !value.is_empty()
            && value.len() <= MAX_MEDIA_TYPE_BYTES
            && value.contains('/')
            && value.bytes().all(|byte| byte.is_ascii_graphic());
        if valid {
            Ok(Self(value.into()))
        } else {
            Err(AssetValueError::InvalidMediaType)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated nonzero pixel dimensions, when decoding supplied them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetDimensions {
    width: NonZeroU64,
    height: NonZeroU64,
}

impl AssetDimensions {
    #[must_use]
    pub const fn new(width: NonZeroU64, height: NonZeroU64) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn width(self) -> NonZeroU64 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> NonZeroU64 {
        self.height
    }
}

/// Exact durable owner of one asset reference.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AssetReferenceOwner {
    CurrentDraftMarker {
        draft_id: SyndicDraftId,
        marker_id: SyndicDraftMarkerId,
    },
    AcceptedInput(SyndicAcceptedInputId),
    SubmittedTurnItem(SyndicItemId),
    QueuedInput(SyndicQueuedInputId),
    RetryRecord(SyndicRetryRecordId),
    TranscriptProjection(SyndicProjectionId),
}

/// Sidecar publication state represented by this first schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetSidecarState {
    Committed,
}

/// Durable metadata for one home-wide content-addressed image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMetadataRecord {
    asset_id: AssetId,
    media_type: AssetMediaType,
    dimensions: Option<AssetDimensions>,
    creation_revision: DomainRevision,
    sidecar_state: AssetSidecarState,
    reference_count: u64,
    revision: RecordRevision,
}

impl AssetMetadataRecord {
    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }
    #[must_use]
    pub const fn media_type(&self) -> &AssetMediaType {
        &self.media_type
    }
    #[must_use]
    pub const fn dimensions(&self) -> Option<AssetDimensions> {
        self.dimensions
    }
    #[must_use]
    pub const fn creation_revision(&self) -> DomainRevision {
        self.creation_revision
    }
    #[must_use]
    pub const fn sidecar_state(&self) -> AssetSidecarState {
        self.sidecar_state
    }
    #[must_use]
    pub const fn reference_count(&self) -> u64 {
        self.reference_count
    }
    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// One immutable typed durable owner-to-asset reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetReferenceRecord {
    owner: AssetReferenceOwner,
    asset_id: AssetId,
    created_at: UnixMillis,
}

impl AssetReferenceRecord {
    #[must_use]
    pub const fn owner(&self) -> AssetReferenceOwner {
        self.owner
    }
    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }
    #[must_use]
    pub const fn created_at(&self) -> UnixMillis {
        self.created_at
    }
}

/// Opaque typed access to durable asset metadata and owner references.
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

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<AssetDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<DomainRevision, ReadError> {
        store.domain_revision(self.handle)
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

    pub fn reference(
        &self,
        store: &HomeStore,
        owner: AssetReferenceOwner,
    ) -> Result<Option<AssetReferenceRecord>, ReadError> {
        store.read_point::<AssetDomain, AssetReferenceCodec>(
            self.handle,
            &owner,
            reference_point_limit(),
        )
    }

    pub fn references_for_asset(
        &self,
        store: &HomeStore,
        asset_id: AssetId,
        after: Option<AssetReferenceOwner>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<AssetReferenceRecord>, ReadError> {
        let start = codec::AssetReferenceIndexKey::minimum(asset_id, after);
        let end = codec::AssetReferenceIndexKey::maximum(asset_id);
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_cursor::<AssetDomain, AssetReferenceIndexCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        Ok(StatePage {
            stored_bytes: page.stored_bytes(),
            has_more: page.has_more(),
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
        })
    }

    pub fn create_with_reference(
        &self,
        expected_revision: DomainRevision,
        sidecar: AdmittedSidecar,
        command: CreateAssetWithReference,
    ) -> Result<FirstAssetContribution, AssetAdmissionError> {
        ensure_sidecar_matches(sidecar.address(), command.asset_id())?;
        let resulting_revision = expected_revision
            .checked_next()
            .map_err(|_| AssetAdmissionError::RevisionExhausted)?;
        if command.creation_revision() != resulting_revision {
            return Err(AssetAdmissionError::CreationRevisionMismatch {
                expected: resulting_revision,
                actual: command.creation_revision(),
            });
        }
        Ok(FirstAssetContribution {
            sidecar,
            contribution: self.handle.contribution(expected_revision, command),
        })
    }

    #[must_use]
    pub fn add_reference(
        &self,
        expected_revision: DomainRevision,
        command: AddAssetReference,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn remove_reference(
        &self,
        expected_revision: DomainRevision,
        command: RemoveAssetReference,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}

/// First-reference contribution that cannot be separated from its retained sidecar token.
pub struct FirstAssetContribution {
    sidecar: AdmittedSidecar,
    contribution: MutationContribution,
}

impl FirstAssetContribution {
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
    if address.length() > MAX_ASSET_BYTES {
        return Err(AssetAdmissionError::ByteBoundExceeded {
            maximum: MAX_ASSET_BYTES,
            actual: address.length(),
        });
    }
    Ok(())
}

fn metadata_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_METADATA_LIMIT + 4).expect("asset metadata limit is nonzero")
}

fn reference_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_REFERENCE_LIMIT + 4).expect("asset reference limit is nonzero")
}
