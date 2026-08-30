use std::collections::BTreeSet;

use beryl_home_store::DomainReader;

use crate::{
    domain::SyndicDomain,
    draft_piece::{
        DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS, DRAFT_MARKER_ADMISSION_MAX_HEADS,
        DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT, DraftMarkerAdmissionCapacityFamily,
        DraftMarkerAdmissionCapacityV1, DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionHeadsFamily, DraftMarkerAdmissionLifecycleV1,
        DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodePayloadV1,
        DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionNodesFamily, DraftMarkerAdmissionOwnerV1,
        DraftMarkerAdmissionReceiptKeyV1, DraftMarkerAdmissionReceiptTransitionV1,
        DraftMarkerAdmissionReceiptsFamily, DraftMarkerAdmissionReplayReceiptV1,
        DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionRootV1,
        DraftMarkerAdmissionTargetDispositionV1, DraftMarkerAdmissionTreeV1,
        encoded_head_record_charge, encoded_node_record_charge, encoded_receipt_record_charge,
    },
    error::SyndicValidationError,
};

use super::scan::{point, scan, scan_range};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut capacity: Option<DraftMarkerAdmissionCapacityV1> = None;
    scan::<DraftMarkerAdmissionCapacityFamily>(reader, |_, value| {
        if capacity.replace(value.clone()).is_some() {
            return invariant("multiple draft-marker admission capacity records");
        }
        Ok(())
    })?;

    let mut aggregate = DraftMarkerAdmissionRetainedChargeV1::ZERO;
    let mut head_count = 0_u64;
    let mut owner_node_records = 0_u64;
    let mut owner_receipt_records = 0_u64;
    scan::<DraftMarkerAdmissionHeadsFamily>(reader, |owner, head| {
        head_count = head_count
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission head count overflow",
            ))?;
        if head_count > DRAFT_MARKER_ADMISSION_MAX_HEADS || head.owner() != *owner {
            return invariant("invalid draft-marker admission head ownership or count");
        }
        validate_head(
            reader,
            *owner,
            head,
            &mut owner_node_records,
            &mut owner_receipt_records,
        )?;
        aggregate =
            aggregate
                .checked_add(head.charge())
                .ok_or(SyndicValidationError::Invariant(
                    "draft-marker admission aggregate overflow",
                ))?;
        Ok(())
    })?;

    let mut all_nodes = 0_u64;
    scan::<DraftMarkerAdmissionNodesFamily>(reader, |key, value| {
        if value.key() != *key {
            return invariant("draft-marker admission node key/value disagreement");
        }
        all_nodes = all_nodes
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission node record count overflow",
            ))?;
        Ok(())
    })?;
    let mut all_receipts = 0_u64;
    scan::<DraftMarkerAdmissionReceiptsFamily>(reader, |key, value| {
        if value.owner() != key.owner() || value.command_id() != key.command_id() {
            return invariant("draft-marker admission receipt key/value disagreement");
        }
        all_receipts = all_receipts
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission receipt record count overflow",
            ))?;
        Ok(())
    })?;

    if all_nodes != owner_node_records || all_receipts != owner_receipt_records {
        return invariant("orphan draft-marker admission node or receipt");
    }
    match capacity {
        None if head_count == 0 && all_nodes == 0 && all_receipts == 0 => Ok(()),
        None => invariant("draft-marker admission records exist without capacity"),
        Some(capacity) if capacity.charge() == aggregate => Ok(()),
        Some(_) => invariant("draft-marker admission capacity aggregate disagreement"),
    }
}

fn validate_head(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: DraftMarkerAdmissionOwnerV1,
    head: &DraftMarkerAdmissionHeadV1,
    aggregate_node_records: &mut u64,
    aggregate_receipt_records: &mut u64,
) -> Result<(), SyndicValidationError> {
    let mut visited = 0_u64;
    let mut current_nodes = BTreeSet::new();
    if head.lifecycle() == DraftMarkerAdmissionLifecycleV1::TerminalCleanup {
        validate_root_owner(owner, head.source_root())?;
        validate_root_owner(owner, head.target_root())?;
    } else {
        let source_unassigned = authenticate_root(
            reader,
            owner,
            head.source_root(),
            &mut visited,
            &mut current_nodes,
        )?;
        let target_unassigned = authenticate_root(
            reader,
            owner,
            head.target_root(),
            &mut visited,
            &mut current_nodes,
        )?;
        if source_unassigned != 0 || target_unassigned != head.unassigned_count() {
            return invariant("draft-marker admission unassigned target count disagreement");
        }
    }

    let first_node = DraftMarkerAdmissionNodeKeyV1::new(
        owner,
        crate::DraftMarkerAdmissionNodeKindV1::Internal,
        crate::DraftMarkerAdmissionNodeIdV1::from_bytes([0; 16]),
    );
    let last_node = DraftMarkerAdmissionNodeKeyV1::new(
        owner,
        crate::DraftMarkerAdmissionNodeKindV1::Leaf,
        crate::DraftMarkerAdmissionNodeIdV1::from_bytes([u8::MAX; 16]),
    );
    let mut encoded_bytes = encoded_head_record_charge(&owner, head).map_err(|_| {
        SyndicValidationError::Invariant("draft-marker admission head charge overflow")
    })?;
    let mut local_nodes = 0_u64;
    let mut local_node_keys = BTreeSet::new();
    let mut local_source_node_keys = BTreeSet::new();
    let mut retained_associations = 0_u64;
    scan_range::<DraftMarkerAdmissionNodesFamily>(reader, first_node, last_node, |key, value| {
        if key.owner() != owner || value.key() != *key {
            return invariant("draft-marker admission node escaped owner range");
        }
        if !local_node_keys.insert(*key) {
            return invariant("duplicate draft-marker admission node key");
        }
        if value.tree() == DraftMarkerAdmissionTreeV1::SourceOrder {
            local_source_node_keys.insert(*key);
        }
        if matches!(
            value.payload(),
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf { .. }
        ) {
            retained_associations =
                retained_associations
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "draft-marker admission retained association count overflow",
                    ))?;
        }
        local_nodes = local_nodes
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission owner node count overflow",
            ))?;
        encoded_bytes = encoded_bytes
            .checked_add(encoded_node_record_charge(key, value).map_err(|_| {
                SyndicValidationError::Invariant("draft-marker admission node charge overflow")
            })?)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission encoded charge overflow",
            ))?;
        Ok(())
    })?;

    let first_receipt = DraftMarkerAdmissionReceiptKeyV1::new(
        owner,
        crate::DraftMarkerAdmissionCommandIdV1::from_bytes([0; 16]),
    );
    let last_receipt = DraftMarkerAdmissionReceiptKeyV1::new(
        owner,
        crate::DraftMarkerAdmissionCommandIdV1::from_bytes([u8::MAX; 16]),
    );
    let mut local_receipts = 0_u64;
    let mut local_receipt = None;
    scan_range::<DraftMarkerAdmissionReceiptsFamily>(
        reader,
        first_receipt,
        last_receipt,
        |key, value| {
            if key.owner() != owner
                || value.owner() != owner
                || value.command_id() != key.command_id()
            {
                return invariant("draft-marker admission receipt escaped owner range");
            }
            local_receipts =
                local_receipts
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "draft-marker admission owner receipt count overflow",
                    ))?;
            if local_receipts > 1 {
                return invariant("multiple draft-marker admission replay receipts for one owner");
            }
            if let Some(selected) = head.selected_receipt() {
                if selected != value.command_id() {
                    return invariant("draft-marker admission head selected a different receipt");
                }
            }
            local_receipt = Some(value.clone());
            encoded_bytes = encoded_bytes
                .checked_add(encoded_receipt_record_charge(key, value).map_err(|_| {
                    SyndicValidationError::Invariant(
                        "draft-marker admission receipt charge overflow",
                    )
                })?)
                .ok_or(SyndicValidationError::Invariant(
                    "draft-marker admission encoded charge overflow",
                ))?;
            Ok(())
        },
    )?;
    let receipt_count_is_valid = match head.selected_receipt() {
        Some(_) => local_receipts == 1,
        None if head.lifecycle() == crate::DraftMarkerAdmissionLifecycleV1::TerminalCleanup => {
            local_receipts == 1
        }
        None => local_receipts == 0,
    };
    if head.lifecycle() == DraftMarkerAdmissionLifecycleV1::TerminalCleanup {
        validate_terminal_membership(
            owner,
            head,
            &local_node_keys,
            &local_source_node_keys,
            local_receipt.as_ref(),
        )?;
    } else {
        validate_live_membership(
            reader,
            owner,
            head,
            &current_nodes,
            &local_node_keys,
            local_receipt.as_ref(),
        )?;
    }
    if !receipt_count_is_valid
        || encoded_bytes != head.charge().encoded_bytes()
        || head.charge().associations() != retained_associations
    {
        return invariant("draft-marker admission head retained charge disagreement");
    }
    *aggregate_node_records =
        aggregate_node_records
            .checked_add(local_nodes)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission aggregate node count overflow",
            ))?;
    *aggregate_receipt_records = aggregate_receipt_records
        .checked_add(local_receipts)
        .ok_or(SyndicValidationError::Invariant(
            "draft-marker admission aggregate receipt count overflow",
        ))?;
    Ok(())
}

fn authenticate_root(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    visited: &mut u64,
    membership: &mut BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
) -> Result<u64, SyndicValidationError> {
    let Some(key) = root.node() else {
        return Ok(0);
    };
    if key.owner() != owner {
        return invariant("draft-marker admission root owner disagreement");
    }
    let node = point::<DraftMarkerAdmissionNodesFamily>(reader, &key)?.ok_or(
        SyndicValidationError::Invariant("draft-marker admission root node is missing"),
    )?;
    if node.tree() != root.tree()
        || node.height() != root.height()
        || node.digest() != root.digest()
        || node.count().map_err(|_| {
            SyndicValidationError::Invariant("draft-marker admission node count overflow")
        })? != root.count()
    {
        return invariant("draft-marker admission root commitment disagreement");
    }
    authenticate_node(reader, owner, root.tree(), &node, true, visited, membership)
}

fn authenticate_node(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: DraftMarkerAdmissionOwnerV1,
    tree: DraftMarkerAdmissionTreeV1,
    node: &DraftMarkerAdmissionNodeV1,
    is_root: bool,
    visited: &mut u64,
    membership: &mut BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
) -> Result<u64, SyndicValidationError> {
    *visited = visited
        .checked_add(1)
        .ok_or(SyndicValidationError::Invariant(
            "draft-marker admission authenticated descent overflow",
        ))?;
    let maximum_visits = DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS
        .checked_mul(4)
        .and_then(|value| value.checked_add(u64::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 2))
        .expect("draft-marker admission validation bound fits u64");
    if *visited > maximum_visits
        || node.key().owner() != owner
        || node.tree() != tree
        || !membership.insert(node.key())
    {
        return invariant("draft-marker admission authenticated descent exceeded its envelope");
    }
    let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = node.payload() else {
        return Ok(u64::from(matches!(
            node.payload(),
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                disposition: DraftMarkerAdmissionTargetDispositionV1::Unassigned,
                ..
            }
        )));
    };
    if (!is_root && children.len() < 2)
        || children.len() > crate::DRAFT_MARKER_ADMISSION_TREE_FANOUT
    {
        return invariant("draft-marker admission non-root fanout disagreement");
    }
    let mut unassigned = 0_u64;
    for child in children.iter() {
        let value = point::<DraftMarkerAdmissionNodesFamily>(reader, &child.key())?.ok_or(
            SyndicValidationError::Invariant("draft-marker admission child node is missing"),
        )?;
        if value.key() != child.key()
            || value.tree() != tree
            || value.height().checked_add(1) != Some(*height)
            || value.digest() != child.digest()
            || value.count().map_err(|_| {
                SyndicValidationError::Invariant("draft-marker admission child count overflow")
            })? != child.count()
            || value.envelope().map_err(|_| {
                SyndicValidationError::Invariant("draft-marker admission child envelope invalid")
            })? != child.envelope()
        {
            return invariant("draft-marker admission child commitment disagreement");
        }
        unassigned = unassigned
            .checked_add(authenticate_node(
                reader, owner, tree, &value, false, visited, membership,
            )?)
            .ok_or(SyndicValidationError::Invariant(
                "draft-marker admission unassigned target count overflow",
            ))?;
    }
    Ok(unassigned)
}

fn validate_receipt_nodes(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: DraftMarkerAdmissionOwnerV1,
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
) -> Result<BTreeSet<DraftMarkerAdmissionNodeKeyV1>, SyndicValidationError> {
    let mut keys = BTreeSet::new();
    for retained in receipt.retained_predecessor_nodes() {
        let node = point::<DraftMarkerAdmissionNodesFamily>(reader, &retained.key())?.ok_or(
            SyndicValidationError::Invariant(
                "draft-marker admission retained replay node is missing",
            ),
        )?;
        if !keys.insert(retained.key())
            || retained.key().owner() != owner
            || node.digest() != retained.digest()
            || node.count().map_err(|_| {
                SyndicValidationError::Invariant("draft-marker admission replay count overflow")
            })? != retained.count()
            || node.envelope().map_err(|_| {
                SyndicValidationError::Invariant("draft-marker admission replay envelope invalid")
            })? != retained.envelope()
        {
            return invariant("draft-marker admission retained replay node disagreement");
        }
    }
    Ok(keys)
}

fn validate_live_membership(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: DraftMarkerAdmissionOwnerV1,
    head: &DraftMarkerAdmissionHeadV1,
    current_nodes: &BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
    local_nodes: &BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
    receipt: Option<&DraftMarkerAdmissionReplayReceiptV1>,
) -> Result<(), SyndicValidationError> {
    let Some(receipt) = receipt else {
        if local_nodes != current_nodes {
            return invariant("orphan draft-marker admission node outside current roots");
        }
        return Ok(());
    };
    if receipt.owner() != owner
        || receipt.request_commitment() != head.request_commitment()
        || receipt.source_after() != head.source_root()
        || receipt.target_after() != head.target_root()
        || !receipt_transition_matches_head(receipt.transition(), head.lifecycle())
    {
        return invariant("draft-marker admission receipt/head disagreement");
    }

    let mut predecessor_nodes = BTreeSet::new();
    let mut predecessor_visited = 0_u64;
    authenticate_root(
        reader,
        owner,
        receipt.source_before(),
        &mut predecessor_visited,
        &mut predecessor_nodes,
    )?;
    authenticate_root(
        reader,
        owner,
        receipt.target_before(),
        &mut predecessor_visited,
        &mut predecessor_nodes,
    )?;
    let exact_predecessor_closure: BTreeSet<_> = predecessor_nodes
        .difference(current_nodes)
        .copied()
        .collect();
    let retained = validate_receipt_nodes(reader, owner, receipt)?;
    if retained != exact_predecessor_closure {
        return invariant("draft-marker admission replay predecessor closure disagreement");
    }
    let exact_membership: BTreeSet<_> = current_nodes
        .union(&exact_predecessor_closure)
        .copied()
        .collect();
    if local_nodes != &exact_membership {
        return invariant("orphan draft-marker admission node outside replay closure");
    }
    validate_assignment_frontier(reader, head)
}

fn validate_terminal_membership(
    owner: DraftMarkerAdmissionOwnerV1,
    head: &DraftMarkerAdmissionHeadV1,
    local_nodes: &BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
    local_source_nodes: &BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
    receipt: Option<&DraftMarkerAdmissionReplayReceiptV1>,
) -> Result<(), SyndicValidationError> {
    let receipt = receipt.ok_or(SyndicValidationError::Invariant(
        "draft-marker admission terminal receipt is missing",
    ))?;
    if receipt.owner() != owner
        || receipt.request_commitment() != head.request_commitment()
        || receipt.source_after() != head.source_root()
        || receipt.target_after() != head.target_root()
        || receipt.transition() != DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup
        || !receipt.retained_predecessor_nodes().is_empty()
    {
        return invariant("draft-marker admission terminal receipt disagreement");
    }
    for root in [receipt.source_before(), receipt.target_before()] {
        validate_root_owner(owner, root)?;
    }
    let cursor = head
        .cleanup_cursor()
        .ok_or(SyndicValidationError::Invariant(
            "draft-marker admission terminal cleanup cursor is missing",
        ))?;
    for key in local_nodes {
        let is_source = local_source_nodes.contains(key);
        let node_is_permitted = match cursor.tree() {
            DraftMarkerAdmissionTreeV1::SourceOrder => {
                if is_source {
                    cursor.after().is_none_or(|after| *key > after)
                } else {
                    true
                }
            }
            DraftMarkerAdmissionTreeV1::TargetId => {
                if is_source {
                    false
                } else {
                    cursor.after().is_none_or(|after| *key > after)
                }
            }
        };
        if !node_is_permitted {
            return invariant("draft-marker admission node precedes terminal cleanup cursor");
        }
    }
    Ok(())
}

fn validate_root_owner(
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
) -> Result<(), SyndicValidationError> {
    if root.node().is_some_and(|key| key.owner() != owner) {
        return invariant("draft-marker admission root owner disagreement");
    }
    Ok(())
}

fn receipt_transition_matches_head(
    transition: DraftMarkerAdmissionReceiptTransitionV1,
    lifecycle: DraftMarkerAdmissionLifecycleV1,
) -> bool {
    matches!(
        (transition, lifecycle),
        (
            DraftMarkerAdmissionReceiptTransitionV1::Ingestion,
            DraftMarkerAdmissionLifecycleV1::Ingesting | DraftMarkerAdmissionLifecycleV1::Assigning
        ) | (
            DraftMarkerAdmissionReceiptTransitionV1::Assignment,
            DraftMarkerAdmissionLifecycleV1::Assigning
        )
    )
}

fn validate_assignment_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftMarkerAdmissionHeadV1,
) -> Result<(), SyndicValidationError> {
    if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Assigning {
        return Ok(());
    }
    let continuation = head
        .assignment_continuation()
        .ok_or(SyndicValidationError::Invariant(
            "draft-marker admission assignment continuation missing",
        ))?;
    let Some((prior_label, prior_asset)) = continuation.prior_source() else {
        return Ok(());
    };
    let (next_label, next_asset) = least_source_occurrence(reader, head.source_root())?.ok_or(
        SyndicValidationError::Invariant("draft-marker admission assigning source root is empty"),
    )?;
    if next_label < prior_label || (next_label == prior_label && next_asset != prior_asset) {
        return invariant("draft-marker admission assignment prior source disagreement");
    }
    Ok(())
}

fn least_source_occurrence(
    reader: &DomainReader<'_, SyndicDomain>,
    root: DraftMarkerAdmissionRootV1,
) -> Result<Option<(beryl_model::ImageLabelOrdinal, beryl_model::AssetId)>, SyndicValidationError> {
    let Some(mut key) = root.node() else {
        return Ok(None);
    };
    loop {
        let node = point::<DraftMarkerAdmissionNodesFamily>(reader, &key)?.ok_or(
            SyndicValidationError::Invariant("draft-marker admission source node is missing"),
        )?;
        match node.payload() {
            DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
                source_key,
                asset_id,
                ..
            } => return Ok(Some((source_key.source_label(), *asset_id))),
            DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } => {
                key = children
                    .first()
                    .ok_or(SyndicValidationError::Invariant(
                        "draft-marker admission source node has no first child",
                    ))?
                    .key();
            }
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf { .. } => {
                return invariant("draft-marker admission source root reached target leaf");
            }
        }
    }
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
