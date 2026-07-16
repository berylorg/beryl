use std::collections::{BTreeMap, BTreeSet};

use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};
use beryl_model::AssetId;

use crate::{RecordRevision, UnixMillis};

use super::{
    AssetDomain, AssetMetadataCodec, AssetMutationError, AssetReferenceCodec,
    AssetReferenceIndexCodec, AssetReferenceIndexKey, AssetReferenceOwner, AssetReferenceRecord,
    ensure_asset_bound, ensure_index_absent, ensure_owner_absent, ensure_revision,
    required_metadata,
};
use crate::asset::{AssetReferenceBatchError, AssetValueError, MAX_ASSET_REFERENCE_BATCH};

/// One exact new durable owner reference in a bounded addition batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetReferenceAddition {
    owner: AssetReferenceOwner,
    asset_id: AssetId,
    expected_record_revision: RecordRevision,
    created_at: UnixMillis,
}

impl AssetReferenceAddition {
    pub fn new(
        owner: AssetReferenceOwner,
        asset_id: AssetId,
        expected_record_revision: RecordRevision,
        created_at: UnixMillis,
    ) -> Result<Self, AssetValueError> {
        ensure_asset_bound(asset_id)?;
        Ok(Self {
            owner,
            asset_id,
            expected_record_revision,
            created_at,
        })
    }

    #[must_use]
    pub const fn owner(&self) -> AssetReferenceOwner {
        self.owner
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn expected_record_revision(&self) -> RecordRevision {
        self.expected_record_revision
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixMillis {
        self.created_at
    }
}

/// One nonempty bounded set of exact destination references added atomically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddAssetReferences {
    additions: Vec<AssetReferenceAddition>,
}

impl AddAssetReferences {
    pub fn new(additions: Vec<AssetReferenceAddition>) -> Result<Self, AssetReferenceBatchError> {
        validate_additions(&additions)?;
        Ok(Self { additions })
    }

    #[must_use]
    pub fn additions(&self) -> &[AssetReferenceAddition] {
        &self.additions
    }
}

impl DomainMutation<AssetDomain> for AddAssetReferences {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        for (asset_id, (expected_revision, count)) in grouped_assets(&self.additions) {
            let metadata = required_metadata(reader, asset_id)?;
            ensure_revision(expected_revision, metadata.revision)?;
            metadata
                .reference_count
                .checked_add(count)
                .ok_or(AssetMutationError::ReferenceCountOverflow)?;
        }
        for addition in &self.additions {
            ensure_owner_absent(reader, addition.owner)?;
            ensure_index_absent(reader, addition.asset_id, addition.owner)?;
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        for (asset_id, (_, count)) in grouped_assets(&self.additions) {
            let mut metadata = required_metadata(reader, asset_id)?;
            metadata.reference_count = metadata
                .reference_count
                .checked_add(count)
                .ok_or(AssetMutationError::ReferenceCountOverflow)?;
            metadata.revision = metadata.revision.checked_next()?;
            mutations.put::<AssetMetadataCodec>(&asset_id, &metadata)?;
        }
        for addition in &self.additions {
            let reference = AssetReferenceRecord {
                owner: addition.owner,
                asset_id: addition.asset_id,
                created_at: addition.created_at,
            };
            mutations.put::<AssetReferenceCodec>(&addition.owner, &reference)?;
            mutations.put::<AssetReferenceIndexCodec>(
                &AssetReferenceIndexKey {
                    asset_id: addition.asset_id,
                    owner: addition.owner,
                },
                &reference,
            )?;
        }
        Ok(())
    }
}

fn validate_additions(
    additions: &[AssetReferenceAddition],
) -> Result<(), AssetReferenceBatchError> {
    validate_count(additions.len())?;
    let mut destinations = BTreeSet::new();
    let mut revisions = BTreeMap::new();
    for addition in additions {
        if !destinations.insert(addition.owner) {
            return Err(AssetReferenceBatchError::DuplicateDestination(
                addition.owner,
            ));
        }
        match revisions.insert(addition.asset_id, addition.expected_record_revision) {
            Some(first) if first != addition.expected_record_revision => {
                return Err(AssetReferenceBatchError::ConflictingRecordRevision {
                    asset_id: addition.asset_id,
                    first,
                    second: addition.expected_record_revision,
                });
            }
            _ => {}
        }
        if addition.expected_record_revision.checked_next().is_err() {
            return Err(AssetReferenceBatchError::RecordRevisionExhausted {
                asset_id: addition.asset_id,
            });
        }
    }
    Ok(())
}

fn validate_count(count: usize) -> Result<(), AssetReferenceBatchError> {
    if count == 0 {
        Err(AssetReferenceBatchError::Empty)
    } else if count > MAX_ASSET_REFERENCE_BATCH {
        Err(AssetReferenceBatchError::TooMany {
            maximum: MAX_ASSET_REFERENCE_BATCH,
            actual: count,
        })
    } else {
        Ok(())
    }
}

pub(in crate::asset) fn grouped_assets(
    additions: &[AssetReferenceAddition],
) -> BTreeMap<AssetId, (RecordRevision, u64)> {
    let mut grouped = BTreeMap::new();
    for addition in additions {
        let entry = grouped
            .entry(addition.asset_id)
            .or_insert((addition.expected_record_revision, 0_u64));
        entry.1 += 1;
    }
    grouped
}
