use std::collections::BTreeSet;

use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};
use beryl_model::AssetId;

use super::{
    AssetDomain, AssetMutationError, AssetReferenceCodec, AssetReferenceIndexCodec,
    AssetReferenceIndexKey, AssetReferenceOwner, AssetReferenceRecord, ensure_asset_bound,
    ensure_index_absent, ensure_owner_absent, read_index, required_metadata, required_reference,
};
use crate::asset::{AssetReferenceBatchError, AssetValueError, MAX_ASSET_REFERENCE_BATCH};

/// One exact source-to-destination durable owner transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetReferenceMove {
    source: AssetReferenceOwner,
    destination: AssetReferenceOwner,
    asset_id: AssetId,
}

impl AssetReferenceMove {
    pub fn new(
        source: AssetReferenceOwner,
        destination: AssetReferenceOwner,
        asset_id: AssetId,
    ) -> Result<Self, AssetValueError> {
        ensure_asset_bound(asset_id)?;
        Ok(Self {
            source,
            destination,
            asset_id,
        })
    }

    #[must_use]
    pub const fn source(&self) -> AssetReferenceOwner {
        self.source
    }

    #[must_use]
    pub const fn destination(&self) -> AssetReferenceOwner {
        self.destination
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }
}

/// One complete nonempty bounded set of exact owner transfers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveAssetReferences {
    moves: Vec<AssetReferenceMove>,
}

impl MoveAssetReferences {
    pub fn new(moves: Vec<AssetReferenceMove>) -> Result<Self, AssetReferenceBatchError> {
        validate_moves(&moves)?;
        Ok(Self { moves })
    }

    #[must_use]
    pub fn moves(&self) -> &[AssetReferenceMove] {
        &self.moves
    }
}

impl DomainMutation<AssetDomain> for MoveAssetReferences {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        let mut assets = BTreeSet::new();
        for reference_move in &self.moves {
            if assets.insert(reference_move.asset_id) {
                let metadata = required_metadata(reader, reference_move.asset_id)?;
                if metadata.reference_count == 0 {
                    return Err(AssetMutationError::ReferenceCountUnderflow);
                }
            }
            let source = required_reference(reader, reference_move.source)?;
            if source.asset_id != reference_move.asset_id {
                return Err(AssetMutationError::ReferenceAssetMismatch);
            }
            let index = read_index(reader, reference_move.asset_id, reference_move.source)?
                .ok_or(AssetMutationError::ReferenceMissing(reference_move.source))?;
            if index != source {
                return Err(AssetMutationError::ReferenceAssetMismatch);
            }
            ensure_owner_absent(reader, reference_move.destination)?;
            ensure_index_absent(reader, reference_move.asset_id, reference_move.destination)?;
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        for reference_move in &self.moves {
            let source = required_reference(reader, reference_move.source)?;
            let destination = AssetReferenceRecord {
                owner: reference_move.destination,
                asset_id: source.asset_id,
                created_at: source.created_at,
            };
            mutations.delete::<AssetReferenceCodec>(&reference_move.source)?;
            mutations.delete::<AssetReferenceIndexCodec>(&AssetReferenceIndexKey {
                asset_id: reference_move.asset_id,
                owner: reference_move.source,
            })?;
            mutations.put::<AssetReferenceCodec>(&reference_move.destination, &destination)?;
            mutations.put::<AssetReferenceIndexCodec>(
                &AssetReferenceIndexKey {
                    asset_id: reference_move.asset_id,
                    owner: reference_move.destination,
                },
                &destination,
            )?;
        }
        Ok(())
    }
}

fn validate_moves(moves: &[AssetReferenceMove]) -> Result<(), AssetReferenceBatchError> {
    if moves.is_empty() {
        return Err(AssetReferenceBatchError::Empty);
    }
    if moves.len() > MAX_ASSET_REFERENCE_BATCH {
        return Err(AssetReferenceBatchError::TooMany {
            maximum: MAX_ASSET_REFERENCE_BATCH,
            actual: moves.len(),
        });
    }

    let mut sources = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    for reference_move in moves {
        if !sources.insert(reference_move.source) {
            return Err(AssetReferenceBatchError::DuplicateSource(
                reference_move.source,
            ));
        }
        if !destinations.insert(reference_move.destination) {
            return Err(AssetReferenceBatchError::DuplicateDestination(
                reference_move.destination,
            ));
        }
    }
    if let Some(owner) = sources.intersection(&destinations).next() {
        return Err(AssetReferenceBatchError::SourceDestinationOverlap(*owner));
    }
    Ok(())
}
