use super::marker_advance::PendingRootAcquisition;
use super::*;
use crate::draft_piece::build_mapping::{
    DraftPieceBuildMappingFamily,
    model::{MapRoot, Node},
};

pub(in crate::draft_piece) fn transition_fragment_key(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> Option<DraftPieceBuildFragmentKeyV1> {
    for receipt in [previous, current] {
        if let Some(active) = receipt.marker_effect_continuation().active() {
            return Some(active.fragment_key());
        }
    }
    for receipt in [previous, current] {
        let ordinal = match receipt.frontier() {
            DraftPieceBuildFrontierV1::Planning { fragment_ordinal }
            | DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal, ..
            }
            | DraftPieceBuildFrontierV1::Applying {
                fragment_ordinal, ..
            }
            | DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal, ..
            } => fragment_ordinal,
            _ => continue,
        };
        return Some(DraftPieceBuildFragmentKeyV1::new(
            receipt.key().draft_id(),
            receipt.key().session_id(),
            receipt.key().operation_id(),
            ordinal,
        ));
    }
    current
        .marker_effect_continuation()
        .scan()
        .scanned_endpoint()
        .map(|endpoint| endpoint.key())
}

pub(in crate::draft_piece) fn pending_root(mapping: DraftPieceBuildMappingV1) -> Option<MapRoot> {
    match mapping.mapping_stage {
        DraftPieceMappingStageV1::DeleteMap { target, .. }
        | DraftPieceMappingStageV1::MapComplete { target, .. } => Some(target),
        DraftPieceMappingStageV1::Ready(ready) => Some(ready.target),
        _ => None,
    }
}

pub(in crate::draft_piece) fn stored_key(
    owner: DraftPieceSettlementKeyV1,
    root: MapRoot,
) -> Result<Option<[u8; 64]>, DraftPiecePrepareErrorV1> {
    if !root.valid() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let MapRoot::Stored(descriptor) = root else {
        return Ok(None);
    };
    let mut key = [0; 64];
    key[..16].copy_from_slice(owner.draft_id().as_bytes());
    key[16..32].copy_from_slice(owner.session_id().as_bytes());
    key[32..48].copy_from_slice(owner.operation_id().as_bytes());
    key[48..].copy_from_slice(&descriptor.id);
    Ok(Some(key))
}

pub(in crate::draft_piece) fn validate_node(
    key: [u8; 64],
    root: MapRoot,
    node: &Node,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let MapRoot::Stored(descriptor) = root else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    if !descriptor.valid()
        || node.key != key
        || !node.shape(true)
        || node.descriptor()? != descriptor
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(())
}

pub(super) fn validate_roots_with(
    acquisition: &mut impl PendingRootAcquisition,
    owner: DraftPieceSettlementKeyV1,
    mapping: Option<DraftPieceBuildMappingV1>,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let mapping = mapping.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    for root in [Some(mapping.current_map), pending_root(mapping)]
        .into_iter()
        .flatten()
    {
        if let Some(key) = stored_key(owner, root)? {
            let node = acquisition.acquire::<DraftPieceBuildMappingFamily>(key)?;
            validate_node(key, root, &node)?;
        }
    }
    Ok(())
}
