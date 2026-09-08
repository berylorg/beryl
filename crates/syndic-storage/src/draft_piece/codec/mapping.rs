use super::super::build_mapping::model::{Descriptor, MapRoot, Measure};
use super::*;
use crate::codec::parts::{dec_opt, enc_opt};

fn enc_unit(e: &mut Encoder, value: u128) {
    e.fixed16(&value.to_be_bytes());
}
fn dec_unit(d: &mut Decoder<'_>) -> Result<u128, CodecError> {
    Ok(u128::from_be_bytes(d.fixed16()?))
}
fn enc_unit_option(e: &mut Encoder, value: Option<u128>) {
    enc_opt(e, value, enc_unit);
}
fn dec_unit_option(d: &mut Decoder<'_>) -> Result<Option<u128>, CodecError> {
    dec_opt(d, "mapping unit option", dec_unit)
}

fn enc_root(e: &mut Encoder, root: MapRoot) {
    match root {
        MapRoot::Empty => e.u8(0),
        MapRoot::Identity(units) => {
            e.u8(1);
            enc_unit(e, units);
        }
        MapRoot::Stored(root) => {
            e.u8(2);
            e.fixed16(&root.id);
            e.fixed32(&root.digest);
            e.u8(root.height);
            enc_unit(e, root.measure.source);
            enc_unit(e, root.measure.target);
        }
    }
}
fn dec_root(d: &mut Decoder<'_>) -> Result<MapRoot, CodecError> {
    let root = match d.u8()? {
        0 => MapRoot::Empty,
        1 => MapRoot::Identity(dec_unit(d)?),
        2 => MapRoot::Stored(Descriptor {
            id: d.fixed16()?,
            digest: d.fixed32()?,
            height: d.u8()?,
            measure: Measure {
                source: dec_unit(d)?,
                target: dec_unit(d)?,
            },
        }),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "mapping root",
                tag,
            });
        }
    };
    if !root.valid() {
        return Err(CodecError::InvalidLength("mapping root"));
    }
    Ok(root)
}
fn enc_fact(e: &mut Encoder, value: DraftPieceMappingSourceFactV1) {
    enc_build_boundary(e, value.boundary);
    enc_unit(e, value.unit);
}
fn dec_fact(d: &mut Decoder<'_>) -> Result<DraftPieceMappingSourceFactV1, CodecError> {
    Ok(DraftPieceMappingSourceFactV1 {
        boundary: dec_build_boundary(d)?,
        unit: dec_unit(d)?,
    })
}
fn enc_proof(e: &mut Encoder, value: DraftPieceMappingProofComponentV1) {
    e.u8(value.component as u8);
    enc_opt(e, value.primary_marker_rank, |e, rank| e.u64(rank));
}
fn dec_proof(d: &mut Decoder<'_>) -> Result<DraftPieceMappingProofComponentV1, CodecError> {
    let component = match d.u8()? {
        0 => DraftPieceMarkerProofComponentV1::Primary,
        1 => DraftPieceMarkerProofComponentV1::Secondary,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "mapping proof component",
                tag,
            });
        }
    };
    let primary_marker_rank = dec_opt(d, "mapping primary rank", |d| d.u64())?;
    if primary_marker_rank.is_some() != (component == DraftPieceMarkerProofComponentV1::Secondary) {
        return Err(CodecError::InvalidLength("mapping proof scratch"));
    }
    Ok(DraftPieceMappingProofComponentV1 {
        component,
        primary_marker_rank,
    })
}
fn dec_kind(d: &mut Decoder<'_>) -> Result<DraftPieceMappingSpliceKindV1, CodecError> {
    Ok(match d.u8()? {
        0 => DraftPieceMappingSpliceKindV1::TextDelete,
        1 => DraftPieceMappingSpliceKindV1::TextInsert,
        2 => DraftPieceMappingSpliceKindV1::MarkerDelete,
        3 => DraftPieceMappingSpliceKindV1::MarkerInsert,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "mapping splice kind",
                tag,
            });
        }
    })
}
fn enc_splice(e: &mut Encoder, value: DraftPieceMappingSpliceV1) {
    e.u8(value.kind as u8);
    enc_unit(e, value.a);
    enc_unit(e, value.removed);
    enc_unit(e, value.inserted);
    enc_opt(e, value.leaf, |e, (id, digest)| {
        e.fixed16(id.as_bytes());
        enc_digest(e, digest);
    });
    e.u64(value.rank);
    e.u64(value.local_start);
    e.u64(value.local_end);
}
fn dec_splice(d: &mut Decoder<'_>) -> Result<DraftPieceMappingSpliceV1, CodecError> {
    Ok(DraftPieceMappingSpliceV1 {
        kind: dec_kind(d)?,
        a: dec_unit(d)?,
        removed: dec_unit(d)?,
        inserted: dec_unit(d)?,
        leaf: dec_opt(d, "mapping splice leaf", |d| {
            Ok((
                DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
                dec_digest(d)?,
            ))
        })?,
        rank: d.u64()?,
        local_start: d.u64()?,
        local_end: d.u64()?,
    })
}
fn enc_ready(e: &mut Encoder, value: DraftPieceMappingReadySpliceV1) {
    e.u8(value.kind as u8);
    enc_unit(e, value.a);
    enc_unit(e, value.removed);
    enc_unit(e, value.inserted);
    enc_root(e, value.target);
}
fn dec_ready(d: &mut Decoder<'_>) -> Result<DraftPieceMappingReadySpliceV1, CodecError> {
    Ok(DraftPieceMappingReadySpliceV1 {
        kind: dec_kind(d)?,
        a: dec_unit(d)?,
        removed: dec_unit(d)?,
        inserted: dec_unit(d)?,
        target: dec_root(d)?,
    })
}

fn enc_stage(e: &mut Encoder, stage: DraftPieceMappingStageV1) {
    use DraftPieceMappingStageV1::*;
    match stage {
        Idle => {
            e.u8(0);
        }
        TextSourceStart { proof } => {
            e.u8(1);
            enc_proof(e, proof);
        }
        TextSourceEnd { start, proof } => {
            e.u8(2);
            enc_fact(e, start);
            enc_proof(e, proof);
        }
        TextPreviousStart { start, end, proof } => {
            e.u8(3);
            enc_fact(e, start);
            enc_fact(e, end);
            enc_proof(e, proof);
        }
        TextPreviousEnd {
            start,
            end,
            previous_start,
            proof,
        } => {
            e.u8(4);
            enc_fact(e, start);
            enc_fact(e, end);
            enc_build_boundary(e, previous_start);
            enc_proof(e, proof);
        }
        TextMapStart { start, end } => {
            e.u8(5);
            enc_fact(e, start);
            enc_fact(e, end);
        }
        TextMapEnd {
            source_start,
            source_end,
            mapped_start,
        } => {
            e.u8(6);
            enc_build_boundary(e, source_start);
            enc_build_boundary(e, source_end);
            enc_unit(e, mapped_start);
        }
        TextResolveStart {
            source_start,
            source_end,
            mapped_start,
            mapped_end,
        } => {
            e.u8(7);
            enc_build_boundary(e, source_start);
            enc_build_boundary(e, source_end);
            enc_unit(e, mapped_start);
            enc_unit(e, mapped_end);
        }
        TextResolveEnd {
            source_start,
            source_end,
            mapped_end,
            start_boundary,
            start_marker_ordinal,
        } => {
            e.u8(8);
            enc_build_boundary(e, source_start);
            enc_build_boundary(e, source_end);
            enc_unit(e, mapped_end);
            enc_build_boundary(e, start_boundary);
            e.u64(start_marker_ordinal);
        }
        MarkerSource {
            removal_source_unit,
        } => {
            e.u8(9);
            enc_unit_option(e, removal_source_unit);
        }
        MarkerMapRemoval { source_unit } => {
            e.u8(10);
            enc_unit(e, source_unit);
        }
        MarkerResolveRemoval { mapped_unit } => {
            e.u8(11);
            enc_unit(e, mapped_unit);
        }
        MarkerWorkingIdentity { mapped_unit } => {
            e.u8(12);
            enc_unit(e, mapped_unit);
        }
        MarkerMapBoundary => {
            e.u8(13);
        }
        MarkerResolveBoundary { mapped_unit } => {
            e.u8(14);
            enc_unit(e, mapped_unit);
        }
        MarkerPlanningReady { boundary } => {
            e.u8(15);
            enc_build_boundary(e, boundary);
        }
        TextDeleteProof => {
            e.u8(16);
        }
        TextInsertProof => {
            e.u8(17);
        }
        DeleteMap {
            splice,
            target,
            remaining_end,
        } => {
            e.u8(18);
            enc_splice(e, splice);
            enc_root(e, target);
            enc_unit(e, remaining_end);
        }
        InsertMap { splice } => {
            e.u8(19);
            enc_splice(e, splice);
        }
        MapComplete { splice, target } => {
            e.u8(20);
            enc_splice(e, splice);
            enc_root(e, target);
        }
        Ready(ready) => {
            e.u8(21);
            enc_ready(e, ready);
        }
        RefreshMap => {
            e.u8(22);
        }
        RefreshSequence { mapped_unit } => {
            e.u8(23);
            enc_unit(e, mapped_unit);
        }
        PublishReady {
            successor_boundary,
            logical_offset,
        } => {
            e.u8(24);
            enc_build_boundary(e, successor_boundary);
            e.u64(logical_offset);
        }
    }
}
fn dec_stage(d: &mut Decoder<'_>) -> Result<DraftPieceMappingStageV1, CodecError> {
    use DraftPieceMappingStageV1::*;
    Ok(match d.u8()? {
        0 => Idle,
        1 => TextSourceStart {
            proof: dec_proof(d)?,
        },
        2 => TextSourceEnd {
            start: dec_fact(d)?,
            proof: dec_proof(d)?,
        },
        3 => TextPreviousStart {
            start: dec_fact(d)?,
            end: dec_fact(d)?,
            proof: dec_proof(d)?,
        },
        4 => TextPreviousEnd {
            start: dec_fact(d)?,
            end: dec_fact(d)?,
            previous_start: dec_build_boundary(d)?,
            proof: dec_proof(d)?,
        },
        5 => TextMapStart {
            start: dec_fact(d)?,
            end: dec_fact(d)?,
        },
        6 => TextMapEnd {
            source_start: dec_build_boundary(d)?,
            source_end: dec_build_boundary(d)?,
            mapped_start: dec_unit(d)?,
        },
        7 => TextResolveStart {
            source_start: dec_build_boundary(d)?,
            source_end: dec_build_boundary(d)?,
            mapped_start: dec_unit(d)?,
            mapped_end: dec_unit(d)?,
        },
        8 => TextResolveEnd {
            source_start: dec_build_boundary(d)?,
            source_end: dec_build_boundary(d)?,
            mapped_end: dec_unit(d)?,
            start_boundary: dec_build_boundary(d)?,
            start_marker_ordinal: d.u64()?,
        },
        9 => MarkerSource {
            removal_source_unit: dec_unit_option(d)?,
        },
        10 => MarkerMapRemoval {
            source_unit: dec_unit(d)?,
        },
        11 => MarkerResolveRemoval {
            mapped_unit: dec_unit(d)?,
        },
        12 => MarkerWorkingIdentity {
            mapped_unit: dec_unit(d)?,
        },
        13 => MarkerMapBoundary,
        14 => MarkerResolveBoundary {
            mapped_unit: dec_unit(d)?,
        },
        15 => MarkerPlanningReady {
            boundary: dec_build_boundary(d)?,
        },
        16 => TextDeleteProof,
        17 => TextInsertProof,
        18 => DeleteMap {
            splice: dec_splice(d)?,
            target: dec_root(d)?,
            remaining_end: dec_unit(d)?,
        },
        19 => InsertMap {
            splice: dec_splice(d)?,
        },
        20 => MapComplete {
            splice: dec_splice(d)?,
            target: dec_root(d)?,
        },
        21 => Ready(dec_ready(d)?),
        22 => RefreshMap,
        23 => RefreshSequence {
            mapped_unit: dec_unit(d)?,
        },
        24 => PublishReady {
            successor_boundary: dec_build_boundary(d)?,
            logical_offset: d.u64()?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "mapping stage",
                tag,
            });
        }
    })
}
pub(super) fn enc_mapping(e: &mut Encoder, mapping: Option<DraftPieceBuildMappingV1>) {
    enc_opt(e, mapping, |e, value| {
        enc_root(e, value.current_map);
        enc_unit(e, value.completed_source_unit);
        enc_unit_option(e, value.fragment_source_end_unit);
        enc_stage(e, value.mapping_stage);
    });
}
pub(super) fn dec_mapping(
    d: &mut Decoder<'_>,
) -> Result<Option<DraftPieceBuildMappingV1>, CodecError> {
    let mapping = dec_opt(d, "build mapping", |d| {
        Ok(DraftPieceBuildMappingV1 {
            current_map: dec_root(d)?,
            completed_source_unit: dec_unit(d)?,
            fragment_source_end_unit: dec_unit_option(d)?,
            mapping_stage: dec_stage(d)?,
        })
    })?;
    if mapping.is_none() {
        return Err(CodecError::InvalidLength("edit mapping required"));
    }
    Ok(mapping)
}
pub(crate) fn canonical_build_mapping_bytes(mapping: Option<DraftPieceBuildMappingV1>) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_mapping(&mut e, mapping);
    e.finish()
}

#[cfg(feature = "test-faults")]
pub(crate) fn mapping_roundtrip_for_test(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = Decoder::new(bytes);
    let mapping = dec_mapping(&mut decoder).ok()?;
    decoder.finish().ok()?;
    Some(canonical_build_mapping_bytes(mapping))
}
