use super::*;

enum EditedSequence {
    Empty,
    Tree(SequenceRef),
    Underfull {
        height: u8,
        children: Vec<DraftPieceChildV1>,
    },
}

fn edit_leaf(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    rank: u64,
    start: usize,
    end: Option<usize>,
    marker: Option<DraftMarkerIdentityOccurrenceV1>,
    prefix_units: u128,
) -> Result<EditedSequence, DraftPiecePrepareErrorV1> {
    if tree.height == 0 {
        if rank != 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let leaf = context.load_sequence_leaf(tree.link)?;
        if let Some(expected) = marker {
            mapping_program::verify_splice(
                context,
                mapping_program::Kind::MarkerDelete,
                prefix_units,
                1,
                0,
            )?;
            return if matches!(leaf.value(), DraftPieceLeafValueV1::Marker(value)
                if value.marker_id() == expected.marker_id() && value.label() == expected.label()
                    && value.asset_id() == expected.asset_id() && value.order_key() == expected.order_key())
                && leaf.key().id() == expected.sequence_leaf_id()
                && leaf.digest() == expected.sequence_leaf_digest()
            {
                Ok(EditedSequence::Empty)
            } else {
                Err(DraftPiecePrepareErrorV1::InvalidRoot)
            };
        }
        let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        };
        let end = end.unwrap_or(text.len());
        if start >= end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
            ));
        }
        if start == 0 && end == text.len() {
            mapping_program::verify_splice(
                context,
                mapping_program::Kind::TextDelete,
                prefix_units,
                (end - start) as u128,
                0,
            )?;
            return Ok(EditedSequence::Empty);
        }
        mapping_program::verify_splice(
            context,
            mapping_program::Kind::TextDelete,
            prefix_units + start as u128,
            (end - start) as u128,
            0,
        )?;
        let mut retained = String::with_capacity(text.len() - (end - start));
        retained.push_str(&text[..start]);
        retained.push_str(&text[end..]);
        return context
            .new_sequence_leaf(DraftPieceLeafValueV1::Text(retained))
            .map(EditedSequence::Tree);
    }

    let node = context.load_sequence_node(tree.link, tree.height, tree.selected_root)?;
    let mut children = node.children().to_vec();
    let mut remaining = rank;
    let mut selected_prefix = prefix_units;
    let index = children
        .iter()
        .position(|child| {
            if remaining < child.piece_count() {
                true
            } else {
                remaining -= child.piece_count();
                selected_prefix +=
                    u128::from(child.logical_utf8_bytes()) + u128::from(child.marker_count());
                false
            }
        })
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let child = SequenceRef {
        link: children[index],
        height: tree.height - 1,
        selected_root: false,
    };
    match edit_leaf(
        context,
        child,
        remaining,
        start,
        end,
        marker,
        selected_prefix,
    )? {
        EditedSequence::Empty => {
            children.remove(index);
        }
        EditedSequence::Tree(replacement) => {
            if replacement.height != child.height {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            children[index] = replacement.link;
        }
        EditedSequence::Underfull {
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
                return Ok(EditedSequence::Underfull {
                    height,
                    children: underfull,
                });
            }
            let sibling_index = if index == 0 { 1 } else { index - 1 };
            let sibling = context.load_sequence_node(children[sibling_index], height, false)?;
            let mut combined = Vec::with_capacity(underfull.len() + sibling.children().len());
            if sibling_index < index {
                combined.extend_from_slice(sibling.children());
                combined.extend(underfull);
            } else {
                combined.extend(underfull);
                combined.extend_from_slice(sibling.children());
            }
            let replacements = if combined.len() <= DRAFT_PIECE_MAX_CHILDREN {
                vec![context.new_sequence_node(height, combined)?.link]
            } else {
                let middle = combined.len() / 2;
                vec![
                    context
                        .new_sequence_node(height, combined[..middle].to_vec())?
                        .link,
                    context
                        .new_sequence_node(height, combined[middle..].to_vec())?
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
        0 => Ok(EditedSequence::Empty),
        1 => Ok(EditedSequence::Underfull {
            height: tree.height,
            children,
        }),
        _ => context
            .new_sequence_node(tree.height, children)
            .map(EditedSequence::Tree),
    }
}

pub(super) fn remove_text_slice(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    start: Boundary,
    end: Boundary,
) -> Result<(Option<SequenceRef>, Boundary, Boundary), DraftPiecePrepareErrorV1> {
    if start >= end {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let (rank, first, last, next_start, next_end) = if end.inner > 0 {
        if end.rank == start.rank {
            (end.rank, start.inner, Some(end.inner), start, start)
        } else {
            (
                end.rank,
                0,
                Some(end.inner),
                start,
                Boundary {
                    rank: end.rank,
                    inner: 0,
                },
            )
        }
    } else {
        let rank = end
            .rank
            .checked_sub(1)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if rank < start.rank {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        if rank == start.rank && start.inner > 0 {
            let insertion = Boundary {
                rank: end.rank,
                inner: 0,
            };
            (rank, start.inner, None, insertion, insertion)
        } else {
            (rank, 0, None, start, Boundary { rank, inner: 0 })
        }
    };
    let sequence = finish_edit(edit_leaf(context, tree, rank, first, last, None, 0)?)?;
    Ok((sequence, next_start, next_end))
}

fn finish_edit(edit: EditedSequence) -> Result<Option<SequenceRef>, DraftPiecePrepareErrorV1> {
    Ok(match edit {
        EditedSequence::Empty => None,
        EditedSequence::Tree(tree) => Some(tree),
        EditedSequence::Underfull { height, children } => {
            if children.len() != 1 || height == 0 {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            Some(SequenceRef {
                link: children[0],
                height: height - 1,
                selected_root: true,
            })
        }
    })
}

pub(super) fn remove_marker(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    rank: u64,
    expected: DraftMarkerIdentityOccurrenceV1,
) -> Result<Option<SequenceRef>, DraftPiecePrepareErrorV1> {
    finish_edit(edit_leaf(context, tree, rank, 0, None, Some(expected), 0)?)
}
