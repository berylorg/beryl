use super::*;

enum EditedIdentity {
    Empty,
    Tree(IndexRef),
    Underfull {
        height: u8,
        children: Vec<DraftMarkerIdentityChildV1>,
    },
}

fn edit_identity(
    context: &mut BuildContext<'_>,
    tree: IndexRef,
    expected: DraftMarkerIdentityOccurrenceV1,
) -> Result<EditedIdentity, DraftPiecePrepareErrorV1> {
    let record = context.load_index_record(tree, tree.selected_root)?;
    if tree.height == 0 {
        return if record.occurrence() == Some(expected) {
            Ok(EditedIdentity::Empty)
        } else {
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        };
    }
    let mut children = record
        .children()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .to_vec();

    let index = children
        .iter()
        .position(|child| {
            child.first() <= expected.marker_id() && expected.marker_id() <= child.last()
        })
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let child = IndexRef {
        link: children[index],
        height: tree.height - 1,
        selected_root: false,
    };
    match edit_identity(context, child, expected)? {
        EditedIdentity::Empty => {
            children.remove(index);
        }
        EditedIdentity::Tree(replacement) => {
            if replacement.height != child.height {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            children[index] = replacement.link;
        }
        EditedIdentity::Underfull {
            height,
            children: underfull,
        } => {
            if height != child.height || underfull.len() != 1 {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            if children.len() == 1 {
                if !tree.selected_root {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                }
                return Ok(EditedIdentity::Underfull {
                    height,
                    children: underfull,
                });
            }
            let sibling_index = if index == 0 { 1 } else { index - 1 };
            let sibling = context.load_index_record(
                IndexRef {
                    link: children[sibling_index],
                    height,
                    selected_root: false,
                },
                false,
            )?;
            let sibling_children = sibling
                .children()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let mut combined = Vec::with_capacity(underfull.len() + sibling_children.len());
            if sibling_index < index {
                combined.extend_from_slice(sibling_children);
                combined.extend(underfull);
            } else {
                combined.extend(underfull);
                combined.extend_from_slice(sibling_children);
            }
            let replacements = if combined.len() <= DRAFT_PIECE_MAX_CHILDREN {
                vec![context.new_index_node(height, combined)?.link]
            } else {
                let middle = combined.len() / 2;
                vec![
                    context
                        .new_index_node(height, combined[..middle].to_vec())?
                        .link,
                    context
                        .new_index_node(height, combined[middle..].to_vec())?
                        .link,
                ]
            };
            children.splice(
                index.min(sibling_index)..=index.max(sibling_index),
                replacements,
            );
        }
    }
    match children.len() {
        0 => Ok(EditedIdentity::Empty),
        1 => Ok(EditedIdentity::Underfull {
            height: tree.height,
            children,
        }),
        _ => context
            .new_index_node(tree.height, children)
            .map(EditedIdentity::Tree),
    }
}

pub(super) fn remove_identity(
    context: &mut BuildContext<'_>,
    tree: IndexRef,
    expected: DraftMarkerIdentityOccurrenceV1,
) -> Result<Option<IndexRef>, DraftPiecePrepareErrorV1> {
    match edit_identity(context, tree, expected)? {
        EditedIdentity::Empty => Ok(None),
        EditedIdentity::Tree(tree) => Ok(Some(tree)),
        EditedIdentity::Underfull { height, children } => {
            if children.len() != 1 || height == 0 {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            if height == 1 {
                context.new_index_node(1, children).map(Some)
            } else {
                Ok(Some(IndexRef {
                    link: children[0],
                    height: height - 1,
                    selected_root: true,
                }))
            }
        }
    }
}

enum EditedOrder {
    Empty,
    Tree(MarkerOrderRef),
    Underfull {
        height: u8,
        children: Vec<DraftMarkerOrderChildV1>,
    },
}

fn edit_order(
    context: &mut BuildContext<'_>,
    tree: MarkerOrderRef,
    rank: u64,
    expected: (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId),
) -> Result<EditedOrder, DraftPiecePrepareErrorV1> {
    let record = context.load_marker_order_record(tree, tree.selected_root)?;
    if tree.height == 0 {
        return if rank == 0 && record.marker() == Some(expected) {
            Ok(EditedOrder::Empty)
        } else {
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        };
    }
    let mut children = record
        .children()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .to_vec();
    let mut remaining = rank;
    let index = children
        .iter()
        .position(|child| {
            if remaining < child.marker_count() {
                true
            } else {
                remaining -= child.marker_count();
                false
            }
        })
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let child = MarkerOrderRef {
        link: children[index],
        height: tree.height - 1,
        selected_root: false,
    };
    match edit_order(context, child, remaining, expected)? {
        EditedOrder::Empty => {
            children.remove(index);
        }
        EditedOrder::Tree(replacement) => {
            if replacement.height != child.height {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            children[index] = replacement.link;
        }
        EditedOrder::Underfull {
            height,
            children: underfull,
        } => {
            if height != child.height || underfull.len() != 1 {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            if children.len() == 1 {
                if !tree.selected_root {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                }
                return Ok(EditedOrder::Underfull {
                    height,
                    children: underfull,
                });
            }
            let sibling_index = if index == 0 { 1 } else { index - 1 };
            let sibling = context.load_marker_order_record(
                MarkerOrderRef {
                    link: children[sibling_index],
                    height,
                    selected_root: false,
                },
                false,
            )?;
            let sibling_children = sibling
                .children()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let mut combined = Vec::with_capacity(underfull.len() + sibling_children.len());
            if sibling_index < index {
                combined.extend_from_slice(sibling_children);
                combined.extend(underfull);
            } else {
                combined.extend(underfull);
                combined.extend_from_slice(sibling_children);
            }
            let replacements = if combined.len() <= DRAFT_PIECE_MAX_CHILDREN {
                vec![context.new_marker_order_node(height, combined)?.link]
            } else {
                let middle = combined.len() / 2;
                vec![
                    context
                        .new_marker_order_node(height, combined[..middle].to_vec())?
                        .link,
                    context
                        .new_marker_order_node(height, combined[middle..].to_vec())?
                        .link,
                ]
            };
            children.splice(
                index.min(sibling_index)..=index.max(sibling_index),
                replacements,
            );
        }
    }
    match children.len() {
        0 => Ok(EditedOrder::Empty),
        1 => Ok(EditedOrder::Underfull {
            height: tree.height,
            children,
        }),
        _ => context
            .new_marker_order_node(tree.height, children)
            .map(EditedOrder::Tree),
    }
}

pub(super) fn remove_order(
    context: &mut BuildContext<'_>,
    tree: MarkerOrderRef,
    rank: u64,
    expected: (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId),
) -> Result<Option<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    match edit_order(context, tree, rank, expected)? {
        EditedOrder::Empty => Ok(None),
        EditedOrder::Tree(tree) => Ok(Some(tree)),
        EditedOrder::Underfull { height, children } => {
            if children.len() != 1 || height == 0 {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            if height == 1 {
                context.new_marker_order_node(1, children).map(Some)
            } else {
                Ok(Some(MarkerOrderRef {
                    link: children[0],
                    height: height - 1,
                    selected_root: true,
                }))
            }
        }
    }
}
