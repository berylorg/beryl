use super::advance_budget::BuildAcquisition;
use super::sequence_advance::required;
use super::*;

pub(in crate::draft_piece) fn previous_proof_fragment_key(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> Option<DraftPieceBuildFragmentKeyV1> {
    [previous, current].into_iter().find_map(|receipt| {
        let active = receipt.marker_effect_continuation().active()?;
        match active.pending() {
            DraftPieceMarkerPendingV1::Proof {
                purpose:
                    DraftPieceMarkerProofPurposeV1::PreviousStart
                    | DraftPieceMarkerProofPurposeV1::PreviousEnd,
                ..
            } => {
                let key = active.fragment_key();
                Some(DraftPieceBuildFragmentKeyV1::new(
                    key.draft_id(),
                    key.session_id(),
                    key.operation_id(),
                    key.ordinal().checked_sub(1)?,
                ))
            }
            _ => None,
        }
    })
}

pub(in crate::draft_piece) fn previous_proof_transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    if matches!(
        current.lifecycle(),
        DraftPieceBuildLifecycleV1::Rejected
            | DraftPieceBuildLifecycleV1::Cancelled
            | DraftPieceBuildLifecycleV1::Error
    ) {
        return previous.marker_effect_continuation() == current.marker_effect_continuation();
    }
    let Some(key) = previous_proof_fragment_key(previous, current) else {
        return true;
    };
    let Some(fragment) = fragment else {
        return false;
    };
    if key.ordinal() == 0
        || fragment.key() != key
        || validate_fragment(fragment.replacement()).is_err()
        || fragment.chain_digest()
            != draft_piece_fragment_chain_link_v1(
                fragment.preceding_chain(),
                key.ordinal(),
                fragment.replacement(),
            )
        || key.ordinal() == 1
            && fragment.preceding_chain() != canonical_empty_draft_piece_fragment_chain_v1()
    {
        return false;
    }
    let prior = previous
        .marker_effect_continuation()
        .active()
        .map(|active| active.pending());
    let next = current
        .marker_effect_continuation()
        .active()
        .map(|active| active.pending());
    let Some(DraftPieceMarkerPendingV1::Proof {
        purpose, component, ..
    }) = prior
    else {
        return !matches!(
            next,
            Some(DraftPieceMarkerPendingV1::Proof {
                component: DraftPieceMarkerProofComponentV1::Secondary,
                ..
            })
        );
    };
    let position = match purpose {
        DraftPieceMarkerProofPurposeV1::PreviousStart => fragment.replacement().start(),
        DraftPieceMarkerProofPurposeV1::PreviousEnd => fragment.replacement().end(),
        _ => return true,
    };
    let composite = matches!(
        position.gap(),
        DraftCompositeGapWitnessV1::AfterAll | DraftCompositeGapWitnessV1::Between { .. }
    );
    let continued = matches!(next, Some(DraftPieceMarkerPendingV1::Proof {
        purpose: next_purpose,
        component: DraftPieceMarkerProofComponentV1::Secondary,
        primary_marker_rank: Some(_),
    }) if next_purpose == purpose);
    match component {
        DraftPieceMarkerProofComponentV1::Primary => continued == composite,
        DraftPieceMarkerProofComponentV1::Secondary => composite && !continued,
    }
}

pub(super) fn validate_pending_roots(
    acquisition: &BuildAcquisition<'_>,
    draft_id: SyndicDraftId,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    validate_pending_roots_with(&mut acquisition.clone(), draft_id, active)
}

pub(super) trait PendingRootAcquisition {
    fn acquire<F: crate::codec::Family>(
        &mut self,
        key: F::Key,
    ) -> Result<F::Value, DraftPiecePrepareErrorV1>;
}

impl PendingRootAcquisition for BuildAcquisition<'_> {
    fn acquire<F: crate::codec::Family>(
        &mut self,
        key: F::Key,
    ) -> Result<F::Value, DraftPiecePrepareErrorV1> {
        required::<F>(self, key)
    }
}

impl PendingRootAcquisition for &DomainReader<'_, SyndicDomain> {
    fn acquire<F: crate::codec::Family>(
        &mut self,
        key: F::Key,
    ) -> Result<F::Value, DraftPiecePrepareErrorV1> {
        super::required::<F>(self, &key).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)
    }
}

impl PendingRootAcquisition for super::super::staging::StagingWindowAcquisitionReader<'_> {
    fn acquire<F: crate::codec::Family>(
        &mut self,
        key: F::Key,
    ) -> Result<F::Value, DraftPiecePrepareErrorV1> {
        self.point::<F>(key)?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)
    }
}

pub(super) fn validate_pending_roots_with(
    acquisition: &mut impl PendingRootAcquisition,
    draft_id: SyndicDraftId,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
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
            let key = DraftPieceRecordKeyV1::new(draft_id, id);
            let node = acquisition.acquire::<DraftPieceNodesFamily>(key)?;
            if node.key() != key {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            validate_sequence_root_node(node, sequence.summary)?;
        } else if sequence.summary.piece_count() != 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    if let Some(identity) = identity {
        if let Some(id) = identity.root_node_id {
            let key = DraftMarkerIdentityRecordKeyV1::new(
                draft_id,
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            );
            let node = acquisition.acquire::<DraftMarkerIdentityIndexFamily>(key)?;
            if node.key() != key {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            validate_index_root_record(node, identity.summary)?;
        } else if identity.summary.record_count() != 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    Ok(())
}

pub(super) fn prepare_consumption(
    acquisition: &BuildAcquisition<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    next: DraftPieceMarkerEffectContinuationV1,
    writer: Option<&DraftMarkerAdmissionHeadV1>,
) -> Result<Option<PreparedDraftMarkerWriterConsumptionV1>, DraftPiecePrepareErrorV1> {
    let Some(active) = build.marker_effect_continuation().active() else {
        return Ok(None);
    };
    if active.phase() != DraftPieceActiveMarkerPhaseV1::Publishing {
        return Ok(None);
    }
    if next.active().is_some()
        || next.scan().completed_effect_count()
            != build
                .marker_effect_continuation()
                .scan()
                .completed_effect_count()
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        || fragment.replacement().marker_effect() != Some(active.effect())
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let Some(admission) = build.writer_admission() else {
        return Ok(None);
    };
    let marker = match active.effect() {
        DraftPieceMarkerEffectV1::Insert(insertion)
        | DraftPieceMarkerEffectV1::Move { insertion, .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. } => insertion.marker(),
        DraftPieceMarkerEffectV1::Remove { .. } => return Ok(None),
    };
    let ordinal = build
        .progress_receipt()
        .key()
        .transition_ordinal()
        .checked_add(1)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let identity =
        draft_marker_writer_consumption_identity_v1(admission.binding().owner(), ordinal)
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    let index = super::super::admission::index::prepare_acquired_marker_consumption(
        acquisition,
        admission.binding().owner(),
        admission.target_root(),
        marker,
        identity,
    )
    .map_err(|error| match error {
        DraftMarkerAdmissionIndexPreparationErrorV1::StoreRead(error) => {
            DraftPiecePrepareErrorV1::Read(error)
        }
        DraftMarkerAdmissionIndexPreparationErrorV1::OperationTooLarge
        | DraftMarkerAdmissionIndexPreparationErrorV1::Schema(
            DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge,
        ) => DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::TreeLimit),
        _ => DraftPiecePrepareErrorV1::InvalidRoot,
    })?;
    let capacity = required::<DraftMarkerAdmissionCapacityFamily>(
        acquisition,
        DraftMarkerAdmissionCapacityKeyV1,
    )?;
    let consumption = seal_draft_marker_writer_consumption_v1(
        admission,
        writer
            .cloned()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        capacity,
        index,
    )
    .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    acquisition
        .budget
        .emission::<DraftMarkerAdmissionHeadsFamily>(
            &consumption.head().owner(),
            consumption.head(),
        )?;
    acquisition
        .budget
        .emission::<DraftMarkerAdmissionCapacityFamily>(
            &DraftMarkerAdmissionCapacityKeyV1,
            consumption.capacity(),
        )?;
    Ok(Some(consumption))
}
