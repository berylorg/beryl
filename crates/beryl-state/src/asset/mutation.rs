use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, PointReadLimit};
use beryl_model::{AssetId, DomainRevision};

use crate::{RecordRevision, UnixMillis};

use super::{
    ASSET_METADATA_LIMIT, ASSET_REFERENCE_LIMIT, AssetDimensions, AssetDomain, AssetMediaType,
    AssetMetadataRecord, AssetMutationError, AssetReferenceOwner, AssetReferenceRecord,
    AssetSidecarState, MAX_ASSET_BYTES,
    codec::{
        AssetMetadataCodec, AssetReferenceCodec, AssetReferenceIndexCodec, AssetReferenceIndexKey,
    },
};

pub(super) mod add_references;
mod move_references;

pub use add_references::{AddAssetReferences, AssetReferenceAddition};
pub use move_references::{AssetReferenceMove, MoveAssetReferences};

/// Create metadata and its first durable owner reference.
pub struct CreateAssetWithReference {
    asset_id: AssetId,
    media_type: AssetMediaType,
    dimensions: Option<AssetDimensions>,
    creation_revision: DomainRevision,
    owner: AssetReferenceOwner,
    created_at: UnixMillis,
}

impl CreateAssetWithReference {
    pub fn new(
        asset_id: AssetId,
        media_type: AssetMediaType,
        dimensions: Option<AssetDimensions>,
        creation_revision: DomainRevision,
        owner: AssetReferenceOwner,
        created_at: UnixMillis,
    ) -> Result<Self, AssetMutationError> {
        ensure_asset_bound(asset_id)?;
        Ok(Self {
            asset_id,
            media_type,
            dimensions,
            creation_revision,
            owner,
            created_at,
        })
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn creation_revision(&self) -> DomainRevision {
        self.creation_revision
    }
}

impl DomainMutation<AssetDomain> for CreateAssetWithReference {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        if read_metadata(reader, self.asset_id)?.is_some() {
            return Err(AssetMutationError::AssetAlreadyExists(self.asset_id));
        }
        ensure_owner_absent(reader, self.owner)?;
        ensure_index_absent(reader, self.asset_id, self.owner)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let metadata = AssetMetadataRecord {
            asset_id: self.asset_id,
            media_type: self.media_type.clone(),
            dimensions: self.dimensions,
            creation_revision: self.creation_revision,
            sidecar_state: AssetSidecarState::Committed,
            reference_count: 1,
            revision: RecordRevision::INITIAL,
        };
        let reference = AssetReferenceRecord {
            owner: self.owner,
            asset_id: self.asset_id,
            created_at: self.created_at,
        };
        mutations.put::<AssetMetadataCodec>(&self.asset_id, &metadata)?;
        mutations.put::<AssetReferenceCodec>(&self.owner, &reference)?;
        mutations.put::<AssetReferenceIndexCodec>(
            &AssetReferenceIndexKey {
                asset_id: self.asset_id,
                owner: self.owner,
            },
            &reference,
        )?;
        Ok(())
    }
}

/// Add one owner reference to existing matching asset metadata.
pub struct AddAssetReference {
    asset_id: AssetId,
    expected_record_revision: RecordRevision,
    owner: AssetReferenceOwner,
    created_at: UnixMillis,
}

impl AddAssetReference {
    pub fn new(
        asset_id: AssetId,
        expected_record_revision: RecordRevision,
        owner: AssetReferenceOwner,
        created_at: UnixMillis,
    ) -> Result<Self, AssetMutationError> {
        ensure_asset_bound(asset_id)?;
        Ok(Self {
            asset_id,
            expected_record_revision,
            owner,
            created_at,
        })
    }
}

impl DomainMutation<AssetDomain> for AddAssetReference {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        let metadata = required_metadata(reader, self.asset_id)?;
        ensure_revision(self.expected_record_revision, metadata.revision)?;
        ensure_owner_absent(reader, self.owner)?;
        ensure_index_absent(reader, self.asset_id, self.owner)?;
        metadata
            .reference_count
            .checked_add(1)
            .ok_or(AssetMutationError::ReferenceCountOverflow)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let mut metadata = required_metadata(reader, self.asset_id)?;
        metadata.reference_count = metadata
            .reference_count
            .checked_add(1)
            .ok_or(AssetMutationError::ReferenceCountOverflow)?;
        metadata.revision = metadata.revision.checked_next()?;
        let reference = AssetReferenceRecord {
            owner: self.owner,
            asset_id: self.asset_id,
            created_at: self.created_at,
        };
        mutations.put::<AssetMetadataCodec>(&self.asset_id, &metadata)?;
        mutations.put::<AssetReferenceCodec>(&self.owner, &reference)?;
        mutations.put::<AssetReferenceIndexCodec>(
            &AssetReferenceIndexKey {
                asset_id: self.asset_id,
                owner: self.owner,
            },
            &reference,
        )?;
        Ok(())
    }
}

/// Remove one durable owner reference without deleting metadata or sidecar bytes.
pub struct RemoveAssetReference {
    owner: AssetReferenceOwner,
    expected_asset_id: AssetId,
    expected_record_revision: RecordRevision,
}

impl RemoveAssetReference {
    #[must_use]
    pub const fn new(
        owner: AssetReferenceOwner,
        expected_asset_id: AssetId,
        expected_record_revision: RecordRevision,
    ) -> Self {
        Self {
            owner,
            expected_asset_id,
            expected_record_revision,
        }
    }
}

impl DomainMutation<AssetDomain> for RemoveAssetReference {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        let reference = required_reference(reader, self.owner)?;
        if reference.asset_id != self.expected_asset_id {
            return Err(AssetMutationError::ReferenceAssetMismatch);
        }
        let metadata = required_metadata(reader, self.expected_asset_id)?;
        ensure_revision(self.expected_record_revision, metadata.revision)?;
        if metadata.reference_count == 0 {
            return Err(AssetMutationError::ReferenceCountUnderflow);
        }
        let index = read_index(reader, self.expected_asset_id, self.owner)?
            .ok_or(AssetMutationError::ReferenceMissing(self.owner))?;
        if index != reference {
            return Err(AssetMutationError::ReferenceAssetMismatch);
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let mut metadata = required_metadata(reader, self.expected_asset_id)?;
        metadata.reference_count = metadata
            .reference_count
            .checked_sub(1)
            .ok_or(AssetMutationError::ReferenceCountUnderflow)?;
        metadata.revision = metadata.revision.checked_next()?;
        mutations.put::<AssetMetadataCodec>(&self.expected_asset_id, &metadata)?;
        mutations.delete::<AssetReferenceCodec>(&self.owner)?;
        mutations.delete::<AssetReferenceIndexCodec>(&AssetReferenceIndexKey {
            asset_id: self.expected_asset_id,
            owner: self.owner,
        })?;
        Ok(())
    }
}

fn read_metadata(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
) -> Result<Option<AssetMetadataRecord>, AssetMutationError> {
    reader
        .point::<AssetMetadataCodec>(&asset_id, metadata_limit())
        .map_err(Into::into)
}

fn required_metadata(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
) -> Result<AssetMetadataRecord, AssetMutationError> {
    read_metadata(reader, asset_id)?.ok_or(AssetMutationError::AssetMissing(asset_id))
}

fn read_reference(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetReferenceOwner,
) -> Result<Option<AssetReferenceRecord>, AssetMutationError> {
    reader
        .point::<AssetReferenceCodec>(&owner, reference_limit())
        .map_err(Into::into)
}

fn required_reference(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetReferenceOwner,
) -> Result<AssetReferenceRecord, AssetMutationError> {
    read_reference(reader, owner)?.ok_or(AssetMutationError::ReferenceMissing(owner))
}

fn read_index(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
    owner: AssetReferenceOwner,
) -> Result<Option<AssetReferenceRecord>, AssetMutationError> {
    reader
        .point::<AssetReferenceIndexCodec>(
            &AssetReferenceIndexKey { asset_id, owner },
            reference_limit(),
        )
        .map_err(Into::into)
}

fn ensure_owner_absent(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetReferenceOwner,
) -> Result<(), AssetMutationError> {
    if read_reference(reader, owner)?.is_some() {
        Err(AssetMutationError::ReferenceAlreadyExists(owner))
    } else {
        Ok(())
    }
}

fn ensure_index_absent(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
    owner: AssetReferenceOwner,
) -> Result<(), AssetMutationError> {
    if read_index(reader, asset_id, owner)?.is_some() {
        Err(AssetMutationError::ReferenceAlreadyExists(owner))
    } else {
        Ok(())
    }
}

fn ensure_revision(
    expected: RecordRevision,
    current: RecordRevision,
) -> Result<(), AssetMutationError> {
    if expected == current {
        Ok(())
    } else {
        Err(AssetMutationError::RecordRevisionConflict { expected, current })
    }
}

fn ensure_asset_bound(asset_id: AssetId) -> Result<(), super::AssetValueError> {
    let actual = asset_id.length().get();
    if actual <= MAX_ASSET_BYTES {
        Ok(())
    } else {
        Err(super::AssetValueError::ByteBoundExceeded {
            maximum: MAX_ASSET_BYTES,
            actual,
        })
    }
}

fn metadata_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_METADATA_LIMIT + 4).expect("asset metadata limit is nonzero")
}

fn reference_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_REFERENCE_LIMIT + 4).expect("asset reference limit is nonzero")
}
