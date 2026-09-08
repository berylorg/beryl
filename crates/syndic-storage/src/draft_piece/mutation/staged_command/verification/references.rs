use super::*;

pub(super) fn build_endpoint(
    reader: &mut OutcomeReader<'_>,
    build: &DraftPieceBuildRecordV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let mut fragments = Vec::with_capacity(4);
    if !build_record_is_exact(build) {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "build canonical shape",
        ));
    }
    let receipt = reader.required::<DraftPieceBuildProgressFamily>(
        build.progress_receipt().key(),
        "build endpoint receipt",
    )?;
    if !progress_receipt_is_exact(&receipt) || !progress_receipt_matches_build(&receipt, build) {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "build endpoint receipt agreement",
        ));
    }
    receipt_effects(reader, &receipt, &mut fragments)?;
    if let Some(previous) = receipt.previous() {
        let prior = reader.required::<DraftPieceBuildProgressFamily>(
            previous.key(),
            "immediate predecessor receipt",
        )?;
        if prior.reference() != previous || !progress_receipt_is_exact(&prior) {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "immediate predecessor receipt agreement",
            ));
        }
        receipt_effects(reader, &prior, &mut fragments)?;
        let scanned_fragment = match receipt
            .marker_effect_continuation()
            .scan()
            .scanned_endpoint()
        {
            Some(endpoint) => {
                let fragment = fragment_reference(
                    reader,
                    &mut fragments,
                    endpoint.key(),
                    "scanned marker fragment",
                )?;
                if canonical_fragment_endpoint(&fragment) != endpoint {
                    return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                        "scanned marker fragment endpoint",
                    ));
                }
                Some(fragment)
            }
            None => None,
        };
        let active = receipt
            .marker_effect_continuation()
            .active()
            .or(prior.marker_effect_continuation().active());
        let fragment = match active {
            Some(active) => Some(fragment_reference(
                reader,
                &mut fragments,
                active.fragment_key(),
                "transition marker fragment",
            )?),
            None => scanned_fragment,
        };
        if !marker_effect_progress_transition_is_exact(&prior, &receipt, fragment.as_ref()) {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "marker effect transition",
            ));
        }
        let previous_fragment =
            super::super::super::marker_advance::previous_proof_fragment_key(&prior, &receipt)
                .map(|key| {
                    fragment_reference(reader, &mut fragments, key, "previous proof fragment")
                })
                .transpose()?;
        if !super::super::super::marker_advance::previous_proof_transition_is_exact(
            &prior,
            &receipt,
            previous_fragment.as_ref(),
        ) || previous_fragment.as_ref().is_some_and(|previous| {
            fragment
                .as_ref()
                .is_none_or(|fragment| previous.chain_digest() != fragment.preceding_chain())
        }) {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "previous marker proof transition",
            ));
        }
    }
    Ok(())
}

fn receipt_effects(
    reader: &mut OutcomeReader<'_>,
    receipt: &DraftPieceBuildProgressReceiptV1,
    fragments: &mut Vec<DraftPieceBuildFragmentV1>,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    if let Some(endpoint) = receipt.fragment_endpoint() {
        let fragment = fragment_reference(
            reader,
            fragments,
            endpoint.key(),
            "receipt fragment endpoint",
        )?;
        if canonical_fragment_endpoint(&fragment) != endpoint {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "receipt fragment endpoint agreement",
            ));
        }
    }
    roots(reader, receipt.key().draft_id(), receipt.working_roots())?;
    if let Some(active) = receipt.marker_effect_continuation().active() {
        let fragment = fragment_reference(
            reader,
            fragments,
            active.fragment_key(),
            "active marker fragment",
        )?;
        if canonical_fragment_endpoint(&fragment).digest() != active.fragment_digest()
            || fragment.replacement().marker_effect() != Some(active.effect())
            || active.source_roots() != receipt.working_roots()
        {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "active marker effect",
            ));
        }
        roots(reader, receipt.key().draft_id(), active.working_roots())?;
        pending_roots(reader, receipt.key().draft_id(), active)?;
    }
    Ok(())
}

fn fragment_reference(
    reader: &mut OutcomeReader<'_>,
    fragments: &mut Vec<DraftPieceBuildFragmentV1>,
    key: DraftPieceBuildFragmentKeyV1,
    label: &'static str,
) -> Result<DraftPieceBuildFragmentV1, StagedDraftPieceOutcomeErrorV1> {
    if let Some(fragment) = fragments.iter().find(|fragment| fragment.key() == key) {
        return Ok(fragment.clone());
    }
    if fragments.len() == 4 {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "build fragment reference bound",
        ));
    }
    let fragment = reader.required::<DraftPieceBuildFragmentsFamily>(key, label)?;
    if fragment.key() != key {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "build fragment reference key",
        ));
    }
    fragments.push(fragment.clone());
    Ok(fragment)
}

fn pending_roots(
    reader: &mut OutcomeReader<'_>,
    draft: SyndicDraftId,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let (sequence, identity) = match active.pending() {
        DraftPieceMarkerPendingV1::RemoveIdentity { sequence_target }
        | DraftPieceMarkerPendingV1::InsertIdentity {
            sequence_target, ..
        } => (Some(sequence_target), None),
        DraftPieceMarkerPendingV1::RemoveOrder {
            sequence_target,
            identity_target,
        }
        | DraftPieceMarkerPendingV1::InsertOrder {
            sequence_target,
            identity_target,
            ..
        } => (Some(sequence_target), Some(identity_target)),
        _ => (None, None),
    };
    if let Some(sequence) = sequence {
        if let Some(id) = sequence.root_node_id {
            let key = DraftPieceRecordKeyV1::new(draft, id);
            let node = reader.required::<DraftPieceNodesFamily>(key, "pending sequence root")?;
            if node.key() != key {
                return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "pending sequence key",
                ));
            }
            validate_sequence_root_node(node, sequence.summary).map_err(|_| {
                StagedDraftPieceOutcomeErrorV1::Invariant("pending sequence summary")
            })?;
        } else if sequence.summary.piece_count() != 0 {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "pending empty sequence",
            ));
        }
    }
    if let Some(identity) = identity {
        if let Some(id) = identity.root_node_id {
            let key = DraftMarkerIdentityRecordKeyV1::new(
                draft,
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            );
            let node =
                reader.required::<DraftMarkerIdentityIndexFamily>(key, "pending identity root")?;
            if node.key() != key {
                return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "pending identity key",
                ));
            }
            validate_index_root_record(node, identity.summary).map_err(|_| {
                StagedDraftPieceOutcomeErrorV1::Invariant("pending identity summary")
            })?;
        } else if identity.summary.record_count() != 0 {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "pending empty identity",
            ));
        }
    }
    Ok(())
}

pub(super) fn combined_root(
    reader: &mut OutcomeReader<'_>,
    root: DraftPieceRootReferenceV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let stored = reader.required::<DraftPieceRootsFamily>(root.key(), "combined root")?;
    if stored.reference() != root {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "combined root reference",
        ));
    }
    roots(
        reader,
        root.key().draft_id(),
        DraftPieceBuildRootsV1::from_root(root),
    )
}

fn roots(
    reader: &mut OutcomeReader<'_>,
    draft: SyndicDraftId,
    roots: DraftPieceBuildRootsV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    if !draft_piece_build_roots_are_locally_exact_v1(roots) {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant("working roots"));
    }
    if let Some(id) = roots.sequence_root() {
        let node = reader.required::<DraftPieceNodesFamily>(
            DraftPieceRecordKeyV1::new(draft, id),
            "sequence root",
        )?;
        validate_sequence_root_node(node, roots.sequence_summary())
            .map_err(|_| StagedDraftPieceOutcomeErrorV1::Invariant("sequence root summary"))?;
    }
    if let Some(id) = roots.marker_index_root() {
        let record = reader.required::<DraftMarkerIdentityIndexFamily>(
            DraftMarkerIdentityRecordKeyV1::new(
                draft,
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            ),
            "identity index root",
        )?;
        validate_index_root_record(record, roots.marker_index_summary()).map_err(|_| {
            StagedDraftPieceOutcomeErrorV1::Invariant("identity index root summary")
        })?;
    }
    if let Some(id) = roots.marker_order_root() {
        let record = reader.required::<DraftMarkerOrderCommitmentsFamily>(
            DraftMarkerOrderRecordKeyV1::new(draft, DraftMarkerOrderRecordKindV1::Internal, id),
            "marker order root",
        )?;
        validate_marker_order_root_record(record, roots)
            .map_err(|_| StagedDraftPieceOutcomeErrorV1::Invariant("marker order root summary"))?;
    }
    Ok(())
}

pub(super) fn admission_root(
    reader: &mut OutcomeReader<'_>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    root.validate_shape()
        .map_err(|_| StagedDraftPieceOutcomeErrorV1::Invariant("admission root shape"))?;
    let Some(key) = root.node() else {
        return Ok(());
    };
    let node = reader.required::<DraftMarkerAdmissionNodesFamily>(key, "admission root node")?;
    if key.owner() != owner
        || node.key() != key
        || node.tree() != root.tree()
        || node.height() != root.height()
        || node.digest() != root.digest()
        || node.count().ok() != Some(root.count())
        || node.validate().is_err()
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "admission root agreement",
        ));
    }
    Ok(())
}
