use super::*;

fn enc_lane(e: &mut Encoder, lane: DraftMutationStagingLaneFrontierV1) {
    e.u64(lane.next_cursor());
    e.u64(lane.next_ordinal());
    e.u64(lane.item_total());
    e.u64(lane.canonical_byte_total());
    enc_digest(e, lane.cumulative_identity());
}

fn dec_lane(d: &mut Decoder<'_>) -> Result<DraftMutationStagingLaneFrontierV1, CodecError> {
    DraftMutationStagingLaneFrontierV1::new(d.u64()?, d.u64()?, d.u64()?, d.u64()?, dec_digest(d)?)
        .ok_or(CodecError::InvalidLength("draft build staging lane"))
}

fn enc_finished(e: &mut Encoder, finished: DraftPieceFinishedStagingReferenceV1) {
    let identity = finished.identity();
    e.fixed16(identity.draft_id().as_bytes());
    e.fixed16(identity.session_id().as_bytes());
    e.fixed16(identity.operation_id().as_bytes());
    enc_digest(e, finished.head_digest());
    let receipt = finished.receipt();
    e.u64(receipt.transition_ordinal());
    enc_digest(e, receipt.digest());
    enc_lane(e, finished.source());
    enc_lane(e, finished.proposal());
}

fn dec_finished(d: &mut Decoder<'_>) -> Result<DraftPieceFinishedStagingReferenceV1, CodecError> {
    let identity = DraftMutationStagingIdentityV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftMutationOperationIdV1::from_bytes(d.fixed16()?),
    );
    let head_digest = dec_digest(d)?;
    let receipt =
        DraftMutationStagingProgressReceiptReferenceV1::new(identity, d.u64()?, dec_digest(d)?)
            .ok_or(CodecError::InvalidLength("draft finished staging receipt"))?;
    Ok(DraftPieceFinishedStagingReferenceV1::new(
        identity,
        head_digest,
        receipt,
        dec_lane(d)?,
        dec_lane(d)?,
    ))
}

fn enc_charges(e: &mut Encoder, charges: DraftPieceMarkerEffectChargesV1) {
    e.u64(charges.logical_utf8_bytes());
    e.u64(charges.marker_count());
    e.u64(charges.encoded_bytes());
}

fn dec_charges(d: &mut Decoder<'_>) -> Result<DraftPieceMarkerEffectChargesV1, CodecError> {
    Ok(DraftPieceMarkerEffectChargesV1::new(
        d.u64()?,
        d.u64()?,
        d.u64()?,
    ))
}

fn enc_occurrence(e: &mut Encoder, occurrence: DraftMarkerIdentityOccurrenceV1) {
    e.fixed16(occurrence.marker_id().as_bytes());
    e.u64(occurrence.label().get());
    enc_asset_id(e, occurrence.asset_id());
    e.u64(occurrence.order_key());
    e.fixed16(occurrence.sequence_leaf_id().as_bytes());
    enc_digest(e, occurrence.sequence_leaf_digest());
}

fn dec_occurrence(d: &mut Decoder<'_>) -> Result<DraftMarkerIdentityOccurrenceV1, CodecError> {
    Ok(DraftMarkerIdentityOccurrenceV1::new(
        SyndicDraftMarkerId::from_bytes(d.fixed16()?),
        ImageLabelOrdinal::new(d.u64()?)
            .map_err(|error| invalid("draft pending-marker label", error))?,
        dec_asset_id(d)?,
        d.u64()?,
        DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
        dec_digest(d)?,
    ))
}

fn enc_removal(e: &mut Encoder, removal: DraftPieceMarkerRemovalProofV1) {
    enc_position(e, removal.position());
    enc_occurrence(e, removal.occurrence());
}

fn dec_removal(d: &mut Decoder<'_>) -> Result<DraftPieceMarkerRemovalProofV1, CodecError> {
    Ok(DraftPieceMarkerRemovalProofV1::new(
        dec_position(d)?,
        dec_occurrence(d)?,
    ))
}

fn enc_insertion(e: &mut Encoder, insertion: DraftPieceMarkerInsertionV1) {
    e.u64(insertion.anchor());
    enc_marker(e, insertion.marker());
    enc_charges(e, insertion.charges());
}

fn dec_insertion(d: &mut Decoder<'_>) -> Result<DraftPieceMarkerInsertionV1, CodecError> {
    Ok(DraftPieceMarkerInsertionV1::new(
        d.u64()?,
        dec_marker(d)?,
        dec_charges(d)?,
    ))
}

pub(super) fn enc_effect(e: &mut Encoder, effect: DraftPieceMarkerEffectV1) {
    match effect {
        DraftPieceMarkerEffectV1::Insert(insertion) => {
            e.u8(0);
            enc_insertion(e, insertion);
        }
        DraftPieceMarkerEffectV1::Remove { removal, charges } => {
            e.u8(1);
            enc_removal(e, removal);
            enc_charges(e, charges);
        }
        DraftPieceMarkerEffectV1::Move { removal, insertion } => {
            e.u8(2);
            enc_removal(e, removal);
            enc_insertion(e, insertion);
        }
        DraftPieceMarkerEffectV1::SameIdReplacement { removal, insertion } => {
            e.u8(3);
            enc_removal(e, removal);
            enc_insertion(e, insertion);
        }
    }
}

pub(super) fn dec_effect(d: &mut Decoder<'_>) -> Result<DraftPieceMarkerEffectV1, CodecError> {
    Ok(match d.u8()? {
        0 => DraftPieceMarkerEffectV1::Insert(dec_insertion(d)?),
        1 => DraftPieceMarkerEffectV1::Remove {
            removal: dec_removal(d)?,
            charges: dec_charges(d)?,
        },
        2 => DraftPieceMarkerEffectV1::Move {
            removal: dec_removal(d)?,
            insertion: dec_insertion(d)?,
        },
        3 => DraftPieceMarkerEffectV1::SameIdReplacement {
            removal: dec_removal(d)?,
            insertion: dec_insertion(d)?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft active marker-effect",
                tag,
            });
        }
    })
}

fn enc_active(e: &mut Encoder, active: DraftPieceActiveMarkerEffectV1) {
    enc_fragment_key(e, active.fragment_key());
    enc_digest(e, active.fragment_digest());
    enc_effect(e, active.effect());
    enc_build_roots(e, active.source_roots());
    enc_build_roots(e, active.working_roots());
    e.u64(active.source_frontier());
    e.u64(active.successor_frontier());
    e.u8(match active.phase() {
        DraftPieceActiveMarkerPhaseV1::Removing => 0,
        DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap => 1,
        DraftPieceActiveMarkerPhaseV1::Inserting => 2,
        DraftPieceActiveMarkerPhaseV1::Publishing => 3,
    });
}

fn dec_active(d: &mut Decoder<'_>) -> Result<DraftPieceActiveMarkerEffectV1, CodecError> {
    let fragment_key = dec_fragment_key(d)?;
    let fragment_digest = dec_digest(d)?;
    let effect = dec_effect(d)?;
    let source_roots = dec_build_roots(d)?;
    let working_roots = dec_build_roots(d)?;
    let source_frontier = d.u64()?;
    let successor_frontier = d.u64()?;
    let phase = match d.u8()? {
        0 => DraftPieceActiveMarkerPhaseV1::Removing,
        1 => DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap,
        2 => DraftPieceActiveMarkerPhaseV1::Inserting,
        3 => DraftPieceActiveMarkerPhaseV1::Publishing,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft active marker-effect phase",
                tag,
            });
        }
    };
    Ok(DraftPieceActiveMarkerEffectV1::new(
        fragment_key,
        fragment_digest,
        effect,
        source_roots,
        working_roots,
        source_frontier,
        successor_frontier,
        phase,
    ))
}

pub(super) fn durable_continuation_is_exact(
    continuation: DraftPieceDurableBuildContinuationV1,
) -> bool {
    continuation.is_locally_exact()
}

pub(super) fn enc_durable_continuation(
    e: &mut Encoder,
    continuation: Option<DraftPieceDurableBuildContinuationV1>,
) {
    let Some(continuation) = continuation else {
        e.u8(0);
        return;
    };
    e.u8(1);
    enc_finished(e, continuation.finished());
    enc_lane(e, continuation.source());
    enc_lane(e, continuation.proposal());
    e.u8(match continuation.phase() {
        DraftPieceBuildStagingPhaseV1::Source => 0,
        DraftPieceBuildStagingPhaseV1::Proposal => 1,
        DraftPieceBuildStagingPhaseV1::Structure => 2,
    });
}

pub(super) fn enc_marker_effect_continuation(
    e: &mut Encoder,
    continuation: DraftPieceMarkerEffectContinuationV1,
) {
    e.u64(continuation.source_logical_frontier());
    e.u64(continuation.successor_logical_frontier());
    let scan = continuation.scan();
    e.u64(scan.next_fragment_ordinal());
    match scan.scanned_endpoint() {
        Some(endpoint) => {
            e.u8(1);
            enc_fragment_key(e, endpoint.key());
            enc_digest(e, endpoint.digest());
            enc_digest(e, endpoint.chain());
        }
        None => e.u8(0),
    }
    e.u64(scan.completed_effect_count());
    enc_digest(e, scan.effect_chain());
    match continuation.active() {
        Some(active) => {
            e.u8(1);
            enc_active(e, active);
        }
        None => e.u8(0),
    }
}

pub(super) fn dec_durable_continuation(
    d: &mut Decoder<'_>,
) -> Result<Option<DraftPieceDurableBuildContinuationV1>, CodecError> {
    let present = match d.u8()? {
        0 => return Ok(None),
        1 => true,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft durable continuation option",
                tag,
            });
        }
    };
    debug_assert!(present);
    let finished = dec_finished(d)?;
    let source = dec_lane(d)?;
    let proposal = dec_lane(d)?;
    let phase = match d.u8()? {
        0 => DraftPieceBuildStagingPhaseV1::Source,
        1 => DraftPieceBuildStagingPhaseV1::Proposal,
        2 => DraftPieceBuildStagingPhaseV1::Structure,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft durable continuation phase",
                tag,
            });
        }
    };
    let continuation = DraftPieceDurableBuildContinuationV1::new(finished, source, proposal, phase);
    if !durable_continuation_is_exact(continuation) {
        return Err(CodecError::InvalidLength("draft durable continuation"));
    }
    Ok(Some(continuation))
}

pub(super) fn dec_marker_effect_continuation(
    d: &mut Decoder<'_>,
) -> Result<DraftPieceMarkerEffectContinuationV1, CodecError> {
    let source_logical_frontier = d.u64()?;
    let successor_logical_frontier = d.u64()?;
    let next_fragment_ordinal = d.u64()?;
    let scanned_endpoint = match d.u8()? {
        0 => None,
        1 => Some(DraftPieceCanonicalFragmentEndpointV1::new(
            dec_fragment_key(d)?,
            dec_digest(d)?,
            dec_digest(d)?,
        )),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft marker-effect scanned endpoint",
                tag,
            });
        }
    };
    let marker_effect_scan = DraftPieceMarkerEffectScanFrontierV1::new(
        next_fragment_ordinal,
        scanned_endpoint,
        d.u64()?,
        dec_digest(d)?,
    );
    let active_marker_effect = match d.u8()? {
        0 => None,
        1 => Some(dec_active(d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft active marker-effect option",
                tag,
            });
        }
    };
    Ok(DraftPieceMarkerEffectContinuationV1::new(
        source_logical_frontier,
        successor_logical_frontier,
        marker_effect_scan,
        active_marker_effect,
    ))
}
