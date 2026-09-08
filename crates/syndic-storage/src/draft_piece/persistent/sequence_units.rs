use super::*;

pub(super) struct UnitFact {
    pub boundary: Boundary,
    pub unit: u128,
    pub logical_offset: u64,
    pub marker_ordinal: u64,
    pub leaf: Option<DraftPieceLeafRecordV1>,
}

#[derive(Clone, Copy)]
enum Cut {
    Unit(u128),
    Boundary(Boundary),
}

pub(super) fn extent(tree: Option<SequenceRef>) -> u128 {
    tree.map_or(0, |tree| {
        u128::from(tree.link.logical_utf8_bytes()) + u128::from(tree.link.marker_count())
    })
}

pub(super) fn at_unit(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    unit: u128,
) -> Result<UnitFact, DraftPiecePrepareErrorV1> {
    locate(context, tree, Cut::Unit(unit))
}

pub(super) fn at_boundary(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    boundary: Boundary,
) -> Result<UnitFact, DraftPiecePrepareErrorV1> {
    locate(context, tree, Cut::Boundary(boundary))
}

fn locate(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    cut: Cut,
) -> Result<UnitFact, DraftPiecePrepareErrorV1> {
    let count = tree.map_or(0, |tree| tree.link.piece_count());
    let units = extent(tree);
    let eof = match cut {
        Cut::Unit(unit) => {
            if unit > units {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            unit == units
        }
        Cut::Boundary(boundary) => {
            if boundary.rank > count || (boundary.rank == count && boundary.inner != 0) {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            boundary.rank == count
        }
    };
    if eof {
        return Ok(UnitFact {
            boundary: Boundary {
                rank: count,
                inner: 0,
            },
            unit: units,
            logical_offset: tree.map_or(0, |tree| tree.link.logical_utf8_bytes()),
            marker_ordinal: tree.map_or(0, |tree| tree.link.marker_count()),
            leaf: None,
        });
    }
    let mut current = tree.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let (mut rank, mut logical, mut markers) = (0u64, 0u64, 0u64);
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut selected = None;
        for child in node.children().iter().copied() {
            let next_rank = rank
                .checked_add(child.piece_count())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let next_logical = logical
                .checked_add(child.logical_utf8_bytes())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let next_markers = markers
                .checked_add(child.marker_count())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let accepts = match cut {
                Cut::Unit(unit) => unit < u128::from(next_logical) + u128::from(next_markers),
                Cut::Boundary(boundary) => boundary.rank < next_rank,
            };
            if accepts {
                selected = Some(child);
                break;
            }
            (rank, logical, markers) = (next_rank, next_logical, next_markers);
        }
        current = SequenceRef {
            link: selected.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            height: current.height - 1,
            selected_root: false,
        };
    }
    let leaf = context.load_sequence_leaf(current.link)?;
    let prefix = u128::from(logical) + u128::from(markers);
    let inner = match cut {
        Cut::Unit(unit) => usize::try_from(
            unit.checked_sub(prefix)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        )
        .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
        Cut::Boundary(boundary) if boundary.rank == rank => boundary.inner,
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    };
    let boundary = match leaf.value() {
        DraftPieceLeafValueV1::Text(text) => {
            if inner > text.len() || !text.is_char_boundary(inner) {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
                ));
            }
            if inner == text.len() {
                Boundary {
                    rank: rank
                        .checked_add(1)
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                    inner: 0,
                }
            } else {
                Boundary { rank, inner }
            }
        }
        DraftPieceLeafValueV1::Marker(_) if inner == 0 => Boundary { rank, inner },
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    };
    Ok(UnitFact {
        boundary,
        unit: prefix + inner as u128,
        logical_offset: logical
            .checked_add(inner as u64)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        marker_ordinal: markers,
        leaf: Some(leaf),
    })
}
