use super::*;
enum RewrittenSubtree {
    Empty,
    Canonical(DraftMarkerAdmissionChildV1, u8),
    Underfull {
        height: u8,
        children: Vec<DraftMarkerAdmissionChildV1>,
    },
}

pub(in super::super) fn rewrite_tree<R: AdmissionNodeReader, F>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    key: SearchKey,
    mut ids: NodeIdFactory,
    replacement: F,
) -> Result<TreeEdit, DraftMarkerAdmissionIndexPreparationErrorV1>
where
    F: FnOnce(
        &DraftMarkerAdmissionNodeV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionSchemaErrorV1>,
{
    let RootPath::Occupied(path) = authenticate_path(ledger, owner, root, key)? else {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode);
    };
    if !path.exact {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode);
    }

    let mut puts = Vec::new();
    let mut path_keys = BTreeSet::new();
    path_keys.insert(path.leaf.key());
    let mut predecessor = path
        .steps
        .iter()
        .map(|step| child(&step.node))
        .collect::<Result<Vec<_>, _>>()?;
    predecessor.push(child(&path.leaf)?);

    let mut rewritten = match replacement(&path.leaf)? {
        Some(template) => {
            let leaf = rebuild_leaf(template, ids.key(DraftMarkerAdmissionNodeKindV1::Leaf)?)?;
            let child = child(&leaf)?;
            emit_node(ledger, leaf, &mut puts)?;
            RewrittenSubtree::Canonical(child, 1)
        }
        None => RewrittenSubtree::Empty,
    };
    if path.steps.is_empty() {
        let root = match rewritten {
            RewrittenSubtree::Empty => canonical_empty_draft_marker_admission_root_v1(root.tree()),
            RewrittenSubtree::Canonical(only, height) => {
                root_from_child(root.tree(), height, only)?
            }
            _ => return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication),
        };
        return Ok(TreeEdit {
            root,
            puts,
            predecessor,
            path_keys,
        });
    }

    for (level, step) in path.steps.iter().rev().enumerate() {
        path_keys.insert(step.node.key());
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = step.node.payload()
        else {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        };
        let mut next = children.to_vec();
        match rewritten {
            RewrittenSubtree::Empty => {
                next.remove(step.child_index);
            }
            RewrittenSubtree::Canonical(replacement, actual_height) => {
                if actual_height.checked_add(1) != Some(*height) {
                    return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
                }
                next[step.child_index] = replacement;
            }
            RewrittenSubtree::Underfull {
                height: child_height,
                children: underfull,
            } => {
                if child_height.checked_add(1) != Some(*height) || underfull.len() != 1 {
                    return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
                }
                if next.len() == 1 {
                    if level + 1 != path.steps.len() {
                        return Err(DraftMarkerAdmissionSchemaErrorV1::NodeFanout.into());
                    }
                    rewritten = RewrittenSubtree::Canonical(underfull[0], child_height - 1);
                    break;
                }
                let sibling_index = if step.child_index > 0 {
                    step.child_index - 1
                } else {
                    1
                };
                let sibling = authenticated_child(
                    ledger,
                    owner,
                    root.tree(),
                    child_height,
                    next[sibling_index],
                )?;
                let DraftMarkerAdmissionNodePayloadV1::Internal {
                    children: sibling_children,
                    ..
                } = sibling.payload()
                else {
                    return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
                };
                if !path_keys.insert(sibling.key()) {
                    return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
                }
                predecessor.push(child(&sibling)?);
                let combined = if sibling_index < step.child_index {
                    sibling_children.iter().copied().chain(underfull).collect()
                } else {
                    underfull
                        .into_iter()
                        .chain(sibling_children.iter().copied())
                        .collect()
                };
                let repaired = make_internal_level(
                    ledger,
                    &mut ids,
                    root.tree(),
                    child_height,
                    combined,
                    &mut puts,
                )?;
                let first = sibling_index.min(step.child_index);
                let last = sibling_index.max(step.child_index);
                next.splice(first..=last, repaired);
            }
        }
        rewritten = if next.is_empty() {
            RewrittenSubtree::Empty
        } else if next.len() == 1 {
            if level + 1 == path.steps.len() {
                RewrittenSubtree::Canonical(next[0], height - 1)
            } else {
                RewrittenSubtree::Underfull {
                    height: *height,
                    children: next,
                }
            }
        } else {
            let replacement =
                make_internal_level(ledger, &mut ids, root.tree(), *height, next, &mut puts)?;
            RewrittenSubtree::Canonical(replacement[0], *height)
        };
    }
    let root = match rewritten {
        RewrittenSubtree::Empty => canonical_empty_draft_marker_admission_root_v1(root.tree()),
        RewrittenSubtree::Canonical(only, height) => {
            if !puts.iter().any(|node| node.key() == only.key()) {
                authenticated_child(ledger, owner, root.tree(), height, only)?;
            }
            root_from_child(root.tree(), height, only)?
        }
        _ => return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication),
    };
    Ok(TreeEdit {
        root,
        puts,
        predecessor,
        path_keys,
    })
}

fn authenticated_child<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    tree: DraftMarkerAdmissionTreeV1,
    height: u8,
    expected: DraftMarkerAdmissionChildV1,
) -> Result<DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionIndexPreparationErrorV1> {
    let node = required_node(ledger, &expected.key())?;
    node.validate()?;
    if node.key().owner() != owner
        || node.tree() != tree
        || node.height() != height
        || child(&node)? != expected
    {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    if let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = node.payload() {
        if children.len() < 2 || children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT {
            return Err(DraftMarkerAdmissionSchemaErrorV1::NodeFanout.into());
        }
    }
    Ok(node)
}
