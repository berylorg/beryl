use super::*;

fn enc_boundary_option(e: &mut Encoder, value: Option<DraftPieceBuildBoundaryV1>) {
    match value {
        None => e.u8(0),
        Some(value) => {
            e.u8(1);
            enc_build_boundary(e, value);
        }
    }
}

fn option_tag(d: &mut Decoder<'_>) -> Result<bool, CodecError> {
    match d.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker program option",
            tag,
        }),
    }
}

fn dec_boundary_option(
    d: &mut Decoder<'_>,
) -> Result<Option<DraftPieceBuildBoundaryV1>, CodecError> {
    if option_tag(d)? {
        Ok(Some(dec_build_boundary(d)?))
    } else {
        Ok(None)
    }
}

fn enc_sequence(e: &mut Encoder, value: DraftPieceSequenceDescriptorV1) {
    enc_record_id_option(e, value.root_node_id);
    enc_summary(e, value.summary);
}

fn dec_sequence(d: &mut Decoder<'_>) -> Result<DraftPieceSequenceDescriptorV1, CodecError> {
    let value = DraftPieceSequenceDescriptorV1 {
        root_node_id: dec_record_id_option(d, "draft marker pending sequence root")?,
        summary: dec_summary(d)?,
    };
    if !value.is_locally_exact() {
        return Err(CodecError::InvalidLength(
            "draft marker sequence descriptor",
        ));
    }
    Ok(value)
}

fn enc_identity(e: &mut Encoder, value: DraftPieceIdentityDescriptorV1) {
    enc_record_id_option(e, value.root_node_id);
    enc_marker_index_summary(e, value.summary);
}

fn dec_identity(d: &mut Decoder<'_>) -> Result<DraftPieceIdentityDescriptorV1, CodecError> {
    let value = DraftPieceIdentityDescriptorV1 {
        root_node_id: dec_record_id_option(d, "draft marker pending identity root")?,
        summary: dec_marker_index_summary(d)?,
    };
    if !value.is_locally_exact() {
        return Err(CodecError::InvalidLength(
            "draft marker identity descriptor",
        ));
    }
    Ok(value)
}

pub(super) fn enc_program(e: &mut Encoder, active: DraftPieceActiveMarkerEffectV1) {
    match active.removal_site() {
        Option::None => e.u8(0),
        Some(site) => {
            e.u8(1);
            e.u64(site.piece_rank);
            e.u64(site.marker_ordinal);
        }
    }
    match active.planning() {
        Option::None => e.u8(0),
        Some(planning) => {
            e.u8(1);
            enc_boundary_option(e, planning.source_boundary);
            enc_boundary_option(e, planning.previous_start);
        }
    }
    match active.insertion_site() {
        Option::None => e.u8(0),
        Some(site) => {
            e.u8(1);
            enc_build_boundary(e, site.boundary);
            e.u64(site.marker_ordinal);
        }
    }
    use DraftPieceMarkerPendingV1::*;
    match active.pending() {
        None => e.u8(0),
        Proof {
            purpose,
            component,
            primary_marker_rank,
        } => {
            e.u8(1);
            e.u8(purpose as u8);
            e.u8(component as u8);
            match primary_marker_rank {
                Option::None => e.u8(0),
                Some(rank) => {
                    e.u8(1);
                    e.u64(rank);
                }
            }
        }
        RemoveSequence => e.u8(2),
        RemoveIdentity { sequence_target } => {
            e.u8(3);
            enc_sequence(e, sequence_target);
        }
        RemoveOrder {
            sequence_target,
            identity_target,
        } => {
            e.u8(4);
            enc_sequence(e, sequence_target);
            enc_identity(e, identity_target);
        }
        InsertSequence => e.u8(5),
        InsertIdentity {
            sequence_target,
            new_leaf_id,
            new_leaf_digest,
        } => {
            e.u8(6);
            enc_sequence(e, sequence_target);
            e.fixed16(new_leaf_id.as_bytes());
            enc_digest(e, new_leaf_digest);
        }
        InsertOrder {
            sequence_target,
            identity_target,
            new_leaf_id,
            new_leaf_digest,
        } => {
            e.u8(7);
            enc_sequence(e, sequence_target);
            enc_identity(e, identity_target);
            e.fixed16(new_leaf_id.as_bytes());
            enc_digest(e, new_leaf_digest);
        }
    }
}

pub(crate) fn canonical_marker_program_bytes(active: DraftPieceActiveMarkerEffectV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_program(&mut e, active);
    e.finish()
}

fn dec_purpose(d: &mut Decoder<'_>) -> Result<DraftPieceMarkerProofPurposeV1, CodecError> {
    use DraftPieceMarkerProofPurposeV1::*;
    Ok(match d.u8()? {
        0 => SourceBounds,
        1 => RemovalGap,
        2 => SourceOccurrence,
        3 => SourceIdentity,
        4 => PreviousStart,
        5 => PreviousEnd,
        8 => InsertIdentityAbsent,
        9 => InsertAnchor,
        10 => InsertOrder,
        11 => InsertAfter,
        12 => SourceInsertIdentityAbsent,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft marker proof purpose",
                tag,
            });
        }
    })
}

pub(super) fn dec_program(
    d: &mut Decoder<'_>,
) -> Result<
    (
        Option<DraftPieceMarkerRemovalSiteV1>,
        Option<DraftPieceMarkerPlanningV1>,
        Option<DraftPieceMarkerInsertionSiteV1>,
        DraftPieceMarkerPendingV1,
    ),
    CodecError,
> {
    let removal = if option_tag(d)? {
        Some(DraftPieceMarkerRemovalSiteV1 {
            piece_rank: d.u64()?,
            marker_ordinal: d.u64()?,
        })
    } else {
        Option::None
    };
    let planning = if option_tag(d)? {
        Some(DraftPieceMarkerPlanningV1 {
            source_boundary: dec_boundary_option(d)?,
            previous_start: dec_boundary_option(d)?,
        })
    } else {
        Option::None
    };
    let insertion = if option_tag(d)? {
        Some(DraftPieceMarkerInsertionSiteV1 {
            boundary: dec_build_boundary(d)?,
            marker_ordinal: d.u64()?,
        })
    } else {
        Option::None
    };
    use DraftPieceMarkerPendingV1::*;
    let pending = match d.u8()? {
        0 => None,
        1 => {
            let purpose = dec_purpose(d)?;
            let component = match d.u8()? {
                0 => DraftPieceMarkerProofComponentV1::Primary,
                1 => DraftPieceMarkerProofComponentV1::Secondary,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft marker proof component",
                        tag,
                    });
                }
            };
            let primary_marker_rank = if option_tag(d)? {
                Some(d.u64()?)
            } else {
                Option::None
            };
            Proof {
                purpose,
                component,
                primary_marker_rank,
            }
        }
        2 => RemoveSequence,
        3 => RemoveIdentity {
            sequence_target: dec_sequence(d)?,
        },
        4 => RemoveOrder {
            sequence_target: dec_sequence(d)?,
            identity_target: dec_identity(d)?,
        },
        5 => InsertSequence,
        6 => InsertIdentity {
            sequence_target: dec_sequence(d)?,
            new_leaf_id: DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
            new_leaf_digest: dec_digest(d)?,
        },
        7 => InsertOrder {
            sequence_target: dec_sequence(d)?,
            identity_target: dec_identity(d)?,
            new_leaf_id: DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
            new_leaf_digest: dec_digest(d)?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft marker pending program",
                tag,
            });
        }
    };
    Ok((removal, planning, insertion, pending))
}
