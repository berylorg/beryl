use super::*;

pub(crate) fn prepare_draft_marker_admission_replay_target_cleanup_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &super::super::DraftMarkerAdmissionHeadV1,
    receipt: &super::super::DraftMarkerAdmissionReplayReceiptV1,
    work: &AdmissionWorkLedger,
) -> Result<
    (Box<[DraftMarkerAdmissionNodeKeyV1]>, u64, u64),
    DraftMarkerAdmissionIndexPreparationErrorV1,
> {
    let owner = head.owner();
    if head.selected_receipt() != Some(receipt.command_id())
        || receipt.owner() != owner
        || receipt.request_commitment() != head.request_commitment()
        || receipt.source_after() != head.source_root()
        || receipt.target_after() != head.target_root()
    {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    let node_reader = DomainAdmissionNodeReader { reader };
    let mut ledger = ReadLedger {
        reader: &node_reader,
        read_bytes: 0,
        work: work.clone().into(),
        cache: BTreeMap::new(),
    };
    let prior_replay_nodes = receipt.retained_predecessor_nodes();
    authenticate_retained_predecessor_nodes(&mut ledger, owner, prior_replay_nodes)?;
    tree_edit::authenticate_receipt_transition(&mut ledger, owner, receipt, Some(head))?;
    let mut retired_targets = Vec::new();
    let mut protected = BTreeSet::new();
    for node in prior_replay_nodes {
        if let DraftMarkerAdmissionEnvelopeV1::TargetId { first, last } = node.envelope() {
            if node.key().kind() == DraftMarkerAdmissionNodeKindV1::Leaf {
                if first != last || node.count() != 1 {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidTree.into());
                }
                let live = tree_edit::exact_target_leaf_key(
                    &mut ledger,
                    owner,
                    head.target_root(),
                    first,
                )?
                .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication)?;
                protected.insert(live);
                retired_targets.push(*node);
            }
        }
    }
    let deletions =
        authenticate_replay_deletions(&mut ledger, owner, &retired_targets, &protected)?;
    let delete_bytes = sum_node_charges(&deletions)?;
    Ok((
        deletions
            .iter()
            .map(DraftMarkerAdmissionNodeV1::key)
            .collect(),
        ledger.read_bytes,
        delete_bytes,
    ))
}
