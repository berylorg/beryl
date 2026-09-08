use super::*;
use crate::draft_piece::{
    DraftMarkerAdmissionReceiptTransitionV1, DraftMarkerAdmissionReplayReceiptV1,
};

mod metadata;
mod summary;

type TransitionError = DraftMarkerAdmissionIndexPreparationErrorV1;

fn invalid() -> TransitionError {
    DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication
}

pub(in super::super) fn authenticate_receipt_transition<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
    selected_head: Option<&DraftMarkerAdmissionHeadV1>,
) -> Result<(), TransitionError> {
    if receipt.owner() != owner {
        return Err(invalid());
    }
    let mut expected = Vec::new();
    match receipt.transition() {
        DraftMarkerAdmissionReceiptTransitionV1::Ingestion => {
            if receipt.source_before().count() != receipt.target_before().count()
                || receipt.source_after().count() != receipt.target_after().count()
            {
                return Err(invalid());
            }
            let Some(entry) = metadata::ingestion(receipt, selected_head)? else {
                return unchanged(receipt);
            };
            if receipt.source_before().count() != receipt.target_before().count()
                || receipt.source_before().count().checked_add(1)
                    != Some(receipt.source_after().count())
                || receipt.target_before().count().checked_add(1)
                    != Some(receipt.target_after().count())
            {
                return Err(invalid());
            }
            let page = DraftMarkerAdmissionPageIdentityV1::new(
                receipt.command_id(),
                receipt.page_ordinal(),
            );
            for (before, after, key, payload) in [
                (
                    receipt.source_before(),
                    receipt.source_after(),
                    SearchKey::Source(entry.source_key),
                    DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
                        source_key: entry.source_key,
                        evidence: entry.evidence.clone(),
                        asset_id: entry.asset,
                    },
                ),
                (
                    receipt.target_before(),
                    receipt.target_after(),
                    SearchKey::Target(entry.source_key.target_marker_id()),
                    DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                        target_marker_id: entry.source_key.target_marker_id(),
                        page,
                        evidence: entry.evidence.clone(),
                        group: entry.source_key.group(),
                        asset_id: entry.asset,
                        disposition: DraftMarkerAdmissionTargetDispositionV1::Unassigned,
                    },
                ),
            ] {
                let ids = NodeIdFactory {
                    owner,
                    page,
                    association_index: entry.association_index,
                    tree: before.tree(),
                    next: 0,
                };
                let actual = summary::insert(
                    ledger,
                    owner,
                    before,
                    after,
                    key,
                    payload,
                    ids,
                    &mut expected,
                )?;
                if actual != after {
                    return Err(invalid());
                }
            }
        }
        DraftMarkerAdmissionReceiptTransitionV1::Assignment => {
            if receipt.source_before().count() == 0 {
                metadata::empty_assignment(receipt)?;
                if receipt.target_before().count() != 0 {
                    return Err(invalid());
                }
                return unchanged(receipt);
            }
            if receipt.source_before().count().checked_sub(1)
                != Some(receipt.source_after().count())
                || receipt.target_before().count() != receipt.target_after().count()
            {
                return Err(invalid());
            }
            let root = cached_root(ledger, owner, receipt.source_before())?;
            let DraftMarkerAdmissionEnvelopeV1::SourceOrder {
                first: source_key, ..
            } = root.envelope()?
            else {
                return Err(invalid());
            };
            let source = retained_path(
                ledger,
                owner,
                receipt.source_before(),
                SearchKey::Source(source_key),
                true,
            )?;
            let source_leaf = cached_node(ledger, source.leaf.key())?;
            let DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
                evidence, asset_id, ..
            } = source_leaf.payload()
            else {
                return Err(invalid());
            };
            let label = metadata::assignment_label(receipt, source_key.group(), *asset_id)?;
            let target = retained_path(
                ledger,
                owner,
                receipt.target_before(),
                SearchKey::Target(source_key.target_marker_id()),
                true,
            )?;
            let target_leaf = cached_node(ledger, target.leaf.key())?;
            let DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                target_marker_id,
                page,
                evidence: target_evidence,
                group,
                asset_id: target_asset,
                disposition,
            } = target_leaf.payload()
            else {
                return Err(invalid());
            };
            if *target_marker_id != source_key.target_marker_id()
                || *group != source_key.group()
                || target_evidence != evidence
                || target_asset != asset_id
                || *disposition != DraftMarkerAdmissionTargetDispositionV1::Unassigned
            {
                return Err(invalid());
            }
            let target_payload = DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                target_marker_id: *target_marker_id,
                page: *page,
                evidence: target_evidence.clone(),
                group: *group,
                asset_id: *target_asset,
                disposition: DraftMarkerAdmissionTargetDispositionV1::Assigned(label),
            };
            let ordinal = receipt
                .target_before()
                .count()
                .checked_sub(receipt.source_before().count())
                .ok_or_else(invalid)?;
            let page =
                DraftMarkerAdmissionPageIdentityV1::new(receipt.command_id(), NonZeroU64::MIN);
            let source_root = summary::rewrite(
                ledger,
                owner,
                receipt.source_before(),
                receipt.source_after(),
                source,
                None,
                NodeIdFactory {
                    owner,
                    page,
                    association_index: ordinal,
                    tree: DraftMarkerAdmissionTreeV1::SourceOrder,
                    next: 0,
                },
                &mut expected,
            )?;
            let target_root = summary::rewrite(
                ledger,
                owner,
                receipt.target_before(),
                receipt.target_after(),
                target,
                Some(target_payload),
                NodeIdFactory {
                    owner,
                    page,
                    association_index: ordinal,
                    tree: DraftMarkerAdmissionTreeV1::TargetId,
                    next: 0,
                },
                &mut expected,
            )?;
            if source_root != receipt.source_after() || target_root != receipt.target_after() {
                return Err(invalid());
            }
        }
        DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup => return Err(invalid()),
    }
    if expected.as_slice() != receipt.retained_predecessor_nodes() {
        return Err(invalid());
    }
    Ok(())
}

fn unchanged(receipt: &DraftMarkerAdmissionReplayReceiptV1) -> Result<(), TransitionError> {
    if receipt.source_before() != receipt.source_after()
        || receipt.target_before() != receipt.target_after()
        || !receipt.retained_predecessor_nodes().is_empty()
    {
        return Err(invalid());
    }
    Ok(())
}

struct RetainedPath {
    steps: Vec<(DraftMarkerAdmissionNodeKeyV1, usize)>,
    leaf: DraftMarkerAdmissionChildV1,
}

fn cached_node<'a, R>(
    ledger: &'a ReadLedger<'_, R>,
    key: DraftMarkerAdmissionNodeKeyV1,
) -> Result<&'a DraftMarkerAdmissionNodeV1, TransitionError> {
    ledger
        .cache
        .get(&key)
        .and_then(Option::as_ref)
        .ok_or_else(invalid)
}

fn cached_root<R>(
    ledger: &ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
) -> Result<DraftMarkerAdmissionNodeV1, TransitionError> {
    validate_profile_root(root)?;
    let node = cached_node(ledger, root.node().ok_or_else(invalid)?)?;
    validate_root_node(node, owner, root)?;
    Ok(node.clone())
}

fn validate_root_node(
    node: &DraftMarkerAdmissionNodeV1,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
) -> Result<(), TransitionError> {
    node.validate()?;
    if node.key().owner() != owner
        || Some(node.key()) != root.node()
        || root_from_node(node)? != root
    {
        return Err(invalid());
    }
    Ok(())
}

fn current_root<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
) -> Result<DraftMarkerAdmissionNodeV1, TransitionError> {
    validate_profile_root(root)?;
    let node = required_node(ledger, &root.node().ok_or_else(invalid)?)?;
    validate_root_node(&node, owner, root)?;
    Ok(node)
}

fn retained_path<R>(
    ledger: &ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    key: SearchKey,
    include_leaf: bool,
) -> Result<RetainedPath, TransitionError> {
    let mut node = cached_root(ledger, owner, root)?;
    let mut steps = Vec::new();
    loop {
        match node.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { height, children } => {
                if (!steps.is_empty() && children.len() < 2)
                    || children
                        .iter()
                        .any(|child| child.count() < (1u64 << (height - 2)))
                {
                    return Err(invalid());
                }
                let index = select_child(children, key)?;
                let expected = children[index];
                steps.push((node.key(), index));
                if *height == 2 && !include_leaf {
                    return Ok(RetainedPath {
                        steps,
                        leaf: expected,
                    });
                }
                let next = cached_node(ledger, expected.key())?;
                next.validate()?;
                if next.tree() != root.tree()
                    || next.height().checked_add(1) != Some(*height)
                    || child(next)? != expected
                {
                    return Err(invalid());
                }
                node = next.clone();
            }
            _ => {
                if leaf_key(&node)? != key {
                    return Err(invalid());
                }
                return Ok(RetainedPath {
                    steps,
                    leaf: child(&node)?,
                });
            }
        }
    }
}

fn append_path<R>(
    ledger: &ReadLedger<'_, R>,
    path: &RetainedPath,
    include_leaf: bool,
    expected: &mut Vec<DraftMarkerAdmissionChildV1>,
) -> Result<(), TransitionError> {
    for (key, _) in &path.steps {
        expected.push(child(cached_node(ledger, *key)?)?);
    }
    if include_leaf {
        expected.push(path.leaf);
    }
    Ok(())
}
