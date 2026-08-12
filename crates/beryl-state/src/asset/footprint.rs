use beryl_home_store::{
    AssetOwnerTransferFootprint, CheckedBatchFootprint, DurableStartFootprintError, RecordCodec,
    participating_domain_footprint,
};

use super::{AssetDomain, codec::AssetOwnerHeadCodec};

/// Returns the owner-derived maximum footprint for draft-to-submitted-item asset transfer.
///
/// The transfer changes at most two compact owner heads: the old draft head and
/// the new submitted-item head. Marker-free input uses a validation-only asset
/// participant and therefore omits this footprint entirely.
pub fn draft_to_submitted_item_owner_transfer_max_footprint()
-> Result<AssetOwnerTransferFootprint, DurableStartFootprintError> {
    AssetOwnerTransferFootprint::draft_to_submitted_item(
        owner_transfer_mutation_footprint()?,
        participating_domain_footprint::<AssetDomain>()?,
    )
}

/// Returns the owner-derived maximum footprint for accepted-input-to-submitted-item transfer.
///
/// Marker-free promotion uses a validation-only asset participant and therefore
/// omits this footprint entirely.
pub fn accepted_input_to_submitted_item_owner_transfer_max_footprint()
-> Result<AssetOwnerTransferFootprint, DurableStartFootprintError> {
    AssetOwnerTransferFootprint::accepted_input_to_submitted_item(
        owner_transfer_mutation_footprint()?,
        participating_domain_footprint::<AssetDomain>()?,
    )
}

fn owner_transfer_mutation_footprint() -> Result<CheckedBatchFootprint, DurableStartFootprintError>
{
    let key_bytes = usize_to_u64(AssetOwnerHeadCodec::MAX_KEY_BYTES)?;
    let value_bytes = AssetOwnerHeadCodec::MAX_VALUE_BYTES
        .checked_add(beryl_home_store::RECORD_VERSION_BYTES)
        .ok_or(DurableStartFootprintError::ArithmeticOverflow)?;
    Ok(CheckedBatchFootprint::new(
        2,
        key_bytes
            .checked_mul(2)
            .ok_or(DurableStartFootprintError::ArithmeticOverflow)?,
        usize_to_u64(value_bytes)?,
    ))
}

fn usize_to_u64(value: usize) -> Result<u64, DurableStartFootprintError> {
    u64::try_from(value).map_err(|_| DurableStartFootprintError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_transfer_covers_source_removal_and_destination_publication() {
        let footprint = owner_transfer_mutation_footprint().expect("checked footprint");
        assert_eq!(2, footprint.records());
        assert_eq!(34, footprint.encoded_key_bytes());
        assert_eq!(516, footprint.encoded_value_bytes());
    }
}
