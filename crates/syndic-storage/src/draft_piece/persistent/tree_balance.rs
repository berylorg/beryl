use super::*;

pub(super) fn pack_children<C: Clone, R>(
    children: Vec<C>,
    mut build: impl FnMut(Vec<C>) -> Result<R, DraftPiecePrepareErrorV1>,
) -> Result<Vec<R>, DraftPiecePrepareErrorV1> {
    if children.len() <= DRAFT_PIECE_MAX_CHILDREN {
        return Ok(vec![build(children)?]);
    }
    let middle = children.len() / 2;
    Ok(vec![
        build(children[..middle].to_vec())?,
        build(children[middle..].to_vec())?,
    ])
}

pub(super) fn rebalance_children<C: Clone>(
    children: &mut Vec<C>,
    index: usize,
    sibling_index: usize,
    underfull: Vec<C>,
    sibling: &[C],
    build: impl FnMut(Vec<C>) -> Result<C, DraftPiecePrepareErrorV1>,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let mut combined = Vec::with_capacity(underfull.len() + sibling.len());
    if sibling_index < index {
        combined.extend_from_slice(sibling);
        combined.extend(underfull);
    } else {
        combined.extend(underfull);
        combined.extend_from_slice(sibling);
    }
    let replacements = pack_children(combined, build)?;
    children.splice(
        index.min(sibling_index)..=index.max(sibling_index),
        replacements,
    );
    Ok(())
}
