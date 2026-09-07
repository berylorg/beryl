use super::*;

pub(crate) fn resolve_assigned_target(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: super::super::DraftMarkerWriterAdmissionV1,
    marker_id: SyndicDraftMarkerId,
    asset_id: AssetId,
    order_key: u64,
) -> Result<DraftPieceMarkerV1, DraftMarkerAdmissionIndexPreparationErrorV1> {
    let reader = StoreAdmissionNodeReader { storage, store };
    let mut read_bytes = 0u64;
    let leaf = point_target_leaf_with(
        |key| {
            let node = reader.point(key)?;
            let charge = match &node {
                Some(node) => encoded_node_record_charge(key, node)?,
                None => encoded_node_key_charge(key)?,
            };
            read_bytes = read_bytes
                .checked_add(charge)
                .filter(|bytes| *bytes <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES)
                .ok_or(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge)?;
            Ok(node)
        },
        admission.binding().owner(),
        admission.target_root(),
        marker_id,
    )?
    .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode)?;
    let DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
        target_marker_id,
        asset_id: assigned_asset,
        group,
        disposition: DraftMarkerAdmissionTargetDispositionV1::Assigned(label),
        ..
    } = leaf.payload()
    else {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    };
    if *target_marker_id != marker_id || *assigned_asset != asset_id {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    if group.allocates()
        && !admission.binding().allocation_range().is_some_and(|range| {
            range.first().get() <= label.get() && label.get() <= range.last().get()
        })
    {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    Ok(DraftPieceMarkerV1::new(
        marker_id, order_key, *label, asset_id,
    ))
}
