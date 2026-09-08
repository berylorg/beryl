use super::*;

fn leaf(
    ids: &mut NodeIdFactory,
    payload: DraftMarkerAdmissionNodePayloadV1,
) -> Result<DraftMarkerAdmissionChildV1, TransitionError> {
    let key = ids.key(DraftMarkerAdmissionNodeKindV1::Leaf)?;
    let envelope = match &payload {
        DraftMarkerAdmissionNodePayloadV1::SourceLeaf { source_key, .. } => {
            DraftMarkerAdmissionEnvelopeV1::SourceOrder {
                first: *source_key,
                last: *source_key,
            }
        }
        DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
            target_marker_id, ..
        } => DraftMarkerAdmissionEnvelopeV1::TargetId {
            first: *target_marker_id,
            last: *target_marker_id,
        },
        _ => return Err(invalid()),
    };
    Ok(DraftMarkerAdmissionChildV1::new(
        key,
        crate::draft_piece::admission::codec::node_digest(key, ids.tree, &payload)?,
        1,
        envelope,
    ))
}

fn level(
    ids: &mut NodeIdFactory,
    height: u8,
    children: Vec<DraftMarkerAdmissionChildV1>,
    generated: &mut BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
) -> Result<Vec<DraftMarkerAdmissionChildV1>, TransitionError> {
    if children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT + 1 {
        return Err(invalid());
    }
    let groups = if children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT {
        let middle = children.len() / 2;
        vec![children[..middle].to_vec(), children[middle..].to_vec()]
    } else {
        vec![children]
    };
    let mut summaries = Vec::with_capacity(groups.len());
    for children in groups {
        if children.is_empty() || height < 2 || height > 18 {
            return Err(invalid());
        }
        let first = children.first().ok_or_else(invalid)?.envelope();
        let last = children.last().ok_or_else(invalid)?.envelope();
        let envelope = match (first, last) {
            (
                DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, .. },
                DraftMarkerAdmissionEnvelopeV1::SourceOrder { last, .. },
            ) => DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, last },
            (
                DraftMarkerAdmissionEnvelopeV1::TargetId { first, .. },
                DraftMarkerAdmissionEnvelopeV1::TargetId { last, .. },
            ) => DraftMarkerAdmissionEnvelopeV1::TargetId { first, last },
            _ => return Err(invalid()),
        };
        let mut count = 0u64;
        for child in &children {
            if child.key().owner() != ids.owner || child.count() < (1u64 << (height - 2)) {
                return Err(invalid());
            }
            count = count
                .checked_add(child.count())
                .filter(|count| *count <= DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS)
                .ok_or_else(invalid)?;
        }
        for pair in children.windows(2) {
            let disjoint = match (pair[0].envelope(), pair[1].envelope()) {
                (
                    DraftMarkerAdmissionEnvelopeV1::SourceOrder { last, .. },
                    DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, .. },
                ) => source_key_less(last, first),
                (
                    DraftMarkerAdmissionEnvelopeV1::TargetId { last, .. },
                    DraftMarkerAdmissionEnvelopeV1::TargetId { first, .. },
                ) => last < first,
                _ => false,
            };
            if !disjoint {
                return Err(invalid());
            }
        }
        let key = ids.key(DraftMarkerAdmissionNodeKindV1::Internal)?;
        let payload = DraftMarkerAdmissionNodePayloadV1::Internal {
            height,
            children: children.into_boxed_slice(),
        };
        let digest = crate::draft_piece::admission::codec::node_digest(key, ids.tree, &payload)?;
        generated.insert(key);
        summaries.push(DraftMarkerAdmissionChildV1::new(
            key, digest, count, envelope,
        ));
    }
    Ok(summaries)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn insert<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    before: DraftMarkerAdmissionRootV1,
    after: DraftMarkerAdmissionRootV1,
    key: SearchKey,
    payload: DraftMarkerAdmissionNodePayloadV1,
    mut ids: NodeIdFactory,
    expected: &mut Vec<DraftMarkerAdmissionChildV1>,
) -> Result<DraftMarkerAdmissionRootV1, TransitionError> {
    validate_profile_root(before)?;
    validate_profile_root(after)?;
    let inserted = leaf(&mut ids, payload)?;
    if before.node().is_none() {
        return Ok(root_from_child(before.tree(), 1, inserted)?);
    }
    let mut generated = BTreeSet::new();
    if before.height() == 1 {
        let root = current_root(ledger, owner, after)?;
        let DraftMarkerAdmissionNodePayloadV1::Internal {
            height: 2,
            children,
        } = root.payload()
        else {
            return Err(invalid());
        };
        let reused = children
            .iter()
            .find(|child| Some(child.key()) == before.node())
            .copied()
            .ok_or_else(invalid)?;
        if root_from_child(before.tree(), 1, reused)? != before
            || reused.key().kind() != DraftMarkerAdmissionNodeKindV1::Leaf
        {
            return Err(invalid());
        }
        let mut children = vec![reused, inserted];
        sort_children(&mut children, before.tree())?;
        let result = level(&mut ids, 2, children, &mut generated)?;
        return Ok(root_from_child(before.tree(), 2, result[0])?);
    }
    let path = retained_path(ledger, owner, before, key, false)?;
    if path.leaf.count() != 1 || path.leaf.key().kind() != DraftMarkerAdmissionNodeKindV1::Leaf {
        return Err(invalid());
    }
    append_path(ledger, &path, false, expected)?;
    let mut replacements = vec![inserted];
    for (level_index, (node_key, index)) in path.steps.iter().rev().enumerate() {
        let node = cached_node(ledger, *node_key)?;
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = node.payload()
        else {
            return Err(invalid());
        };
        let mut children = children.to_vec();
        if level_index == 0 {
            children.extend(replacements);
            sort_children(&mut children, before.tree())?;
        } else {
            children.splice(*index..=*index, replacements);
        }
        replacements = level(&mut ids, *height, children, &mut generated)?;
    }
    match replacements.as_slice() {
        [only] => Ok(root_from_child(before.tree(), before.height(), *only)?),
        [left, right] => {
            let height = before.height().checked_add(1).ok_or_else(invalid)?;
            let result = level(&mut ids, height, vec![*left, *right], &mut generated)?;
            Ok(root_from_child(before.tree(), height, result[0])?)
        }
        _ => Err(invalid()),
    }
}

enum Rewritten {
    Empty,
    Canonical(DraftMarkerAdmissionChildV1, u8),
    Underfull(u8, Vec<DraftMarkerAdmissionChildV1>),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn rewrite<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    before: DraftMarkerAdmissionRootV1,
    after: DraftMarkerAdmissionRootV1,
    path: RetainedPath,
    replacement: Option<DraftMarkerAdmissionNodePayloadV1>,
    mut ids: NodeIdFactory,
    expected: &mut Vec<DraftMarkerAdmissionChildV1>,
) -> Result<DraftMarkerAdmissionRootV1, TransitionError> {
    append_path(ledger, &path, true, expected)?;
    let mut generated = BTreeSet::new();
    let mut rewritten = match replacement {
        Some(payload) => {
            let child = leaf(&mut ids, payload)?;
            generated.insert(child.key());
            Rewritten::Canonical(child, 1)
        }
        None => Rewritten::Empty,
    };
    for (level_index, (node_key, index)) in path.steps.iter().rev().enumerate() {
        let node = cached_node(ledger, *node_key)?;
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = node.payload()
        else {
            return Err(invalid());
        };
        let mut next = children.to_vec();
        match rewritten {
            Rewritten::Empty => {
                next.remove(*index);
            }
            Rewritten::Canonical(child, actual_height) => {
                if actual_height.checked_add(1) != Some(*height) {
                    return Err(invalid());
                }
                next[*index] = child;
            }
            Rewritten::Underfull(child_height, underfull) => {
                if child_height.checked_add(1) != Some(*height) || underfull.len() != 1 {
                    return Err(invalid());
                }
                if next.len() == 1 {
                    if level_index + 1 != path.steps.len() {
                        return Err(invalid());
                    }
                    rewritten = Rewritten::Canonical(underfull[0], child_height - 1);
                    break;
                }
                let sibling_index = if *index > 0 { index - 1 } else { 1 };
                let sibling = cached_node(ledger, next[sibling_index].key())?;
                sibling.validate()?;
                let DraftMarkerAdmissionNodePayloadV1::Internal {
                    height: sibling_height,
                    children: siblings,
                } = sibling.payload()
                else {
                    return Err(invalid());
                };
                if *sibling_height != child_height
                    || sibling.tree() != before.tree()
                    || siblings.len() < 2
                    || child(sibling)? != next[sibling_index]
                {
                    return Err(invalid());
                }
                expected.push(child(sibling)?);
                let combined = if sibling_index < *index {
                    siblings.iter().copied().chain(underfull).collect()
                } else {
                    underfull
                        .into_iter()
                        .chain(siblings.iter().copied())
                        .collect()
                };
                let repaired = level(&mut ids, child_height, combined, &mut generated)?;
                next.splice(
                    sibling_index.min(*index)..=sibling_index.max(*index),
                    repaired,
                );
            }
        }
        rewritten = match next.len() {
            0 => Rewritten::Empty,
            1 if level_index + 1 == path.steps.len() => Rewritten::Canonical(next[0], height - 1),
            1 => Rewritten::Underfull(*height, next),
            _ => {
                let replacement = level(&mut ids, *height, next, &mut generated)?;
                Rewritten::Canonical(replacement[0], *height)
            }
        };
    }
    let result = match rewritten {
        Rewritten::Empty => canonical_empty_draft_marker_admission_root_v1(before.tree()),
        Rewritten::Canonical(child, height) => root_from_child(before.tree(), height, child)?,
        Rewritten::Underfull(..) => return Err(invalid()),
    };
    if result != after {
        return Err(invalid());
    }
    if let Some(key) = result.node() {
        if !generated.contains(&key) {
            let root = current_root(ledger, owner, after)?;
            if let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = root.payload() {
                if children.len() < 2 {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(result)
}
