use super::super::mapping_program::{self, Kind, Stage};
use super::*;

pub(super) fn advance(
    context: &mut BuildContext<'_>,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<Option<DraftPieceActiveMarkerEffectV1>, DraftPiecePrepareErrorV1> {
    if active.pending() != Pending::None {
        return Ok(None);
    }
    let mapping = mapping_program::mapping(context)?;
    let mut next_active = active;
    let next = match mapping.mapping_stage {
        Stage::MarkerSource {
            removal_source_unit,
        } => match removal_source_unit {
            Some(source_unit) if removal(active.effect()).is_some() => {
                Stage::MarkerMapRemoval { source_unit }
            }
            None if removal(active.effect()).is_none() => Stage::MarkerMapBoundary,
            _ => return invalid(),
        },
        Stage::MarkerMapRemoval { source_unit } => Stage::MarkerResolveRemoval {
            mapped_unit: mapping_program::source_cut(context, source_unit, true)?,
        },
        Stage::MarkerResolveRemoval { mapped_unit } => {
            let (sequence, _, _) = load_working_roots(context, active.working_roots())?;
            let fact = sequence_units::at_unit(context, sequence, mapped_unit)?;
            let expected = removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .occurrence();
            let leaf = fact.leaf.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            if fact.boundary.inner != 0
                || leaf.key().id() != expected.sequence_leaf_id()
                || leaf.digest() != expected.sequence_leaf_digest()
                || !matches!(leaf.value(), DraftPieceLeafValueV1::Marker(marker) if marker.marker_id() == expected.marker_id() && marker.order_key() == expected.order_key() && marker.label() == expected.label() && marker.asset_id() == expected.asset_id())
            {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::Overlap,
                ));
            }
            validate_marker_effect_charge(active.effect(), &leaf)?;
            next_active = update(
                active,
                active.working_roots(),
                active.phase(),
                Some(DraftPieceMarkerRemovalSiteV1 {
                    piece_rank: fact.boundary.rank,
                    marker_ordinal: fact.marker_ordinal,
                }),
                active.planning(),
                None,
                Pending::None,
            );
            Stage::MarkerWorkingIdentity { mapped_unit }
        }
        Stage::MarkerWorkingIdentity { mapped_unit } => {
            let (_, identity, _) = load_working_roots(context, active.working_roots())?;
            let expected = removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .occurrence();
            if index_lookup(context, identity, expected.marker_id())? != Some(expected) {
                return invalid();
            }
            let site = active
                .removal_site()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            mapping_program::begin_delete(
                context,
                DraftPieceMappingSpliceV1 {
                    kind: Kind::MarkerDelete,
                    a: mapped_unit,
                    removed: 1,
                    inserted: 0,
                    leaf: None,
                    rank: site.piece_rank,
                    local_start: 0,
                    local_end: 0,
                },
            )?;
            return Ok(Some(active));
        }
        Stage::MarkerMapBoundary => Stage::MarkerResolveBoundary {
            mapped_unit: mapping_program::source_cut(
                context,
                mapping
                    .fragment_source_end_unit
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                false,
            )?,
        },
        Stage::MarkerResolveBoundary { mapped_unit } => {
            let (sequence, _, _) = load_working_roots(context, active.working_roots())?;
            let fact = sequence_units::at_unit(context, sequence, mapped_unit)?;
            Stage::MarkerPlanningReady {
                boundary: durable_boundary(fact.boundary),
            }
        }
        _ => return Ok(None),
    };
    mapping_program::set_stage(context, next)?;
    Ok(Some(next_active))
}

pub(super) fn install_map(
    context: &mut BuildContext<'_>,
    roots: DraftPieceBuildRootsV1,
    inserted: bool,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let Stage::Ready(ready) = mapping_program::mapping(context)?.mapping_stage else {
        return invalid();
    };
    if ready.target.measure().target
        != u128::from(roots.sequence_summary().logical_utf8_bytes())
            + u128::from(roots.sequence_summary().marker_count())
    {
        return invalid();
    }
    mapping_program::consume_ready(
        context,
        if inserted {
            Kind::MarkerInsert
        } else {
            Kind::MarkerDelete
        },
    )?;
    mapping_program::set_stage(
        context,
        if inserted {
            Stage::RefreshMap
        } else {
            Stage::MarkerMapBoundary
        },
    )
}
