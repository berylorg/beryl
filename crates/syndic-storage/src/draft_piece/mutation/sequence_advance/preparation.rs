use super::*;

#[inline(never)]
pub(in super::super) fn prepare(
    acquisition: BuildAcquisition<'_>,
    expected_revision: DomainRevision,
    build: Box<DraftPieceBuildRecordV1>,
) -> Result<PreparedDraftPieceAdvanceV1, DraftPiecePrepareErrorV1> {
    let receipt = authenticate_receipt(&acquisition, &build)?;
    let prepared = prepare_transition(&acquisition, expected_revision, build, receipt)?;
    validate_target_records(&acquisition, &prepared)?;
    Ok(prepared)
}

#[inline(never)]
fn authenticate_receipt(
    acquisition: &BuildAcquisition<'_>,
    build: &DraftPieceBuildRecordV1,
) -> Result<Box<DraftPieceBuildProgressReceiptV1>, DraftPiecePrepareErrorV1> {
    if !build_record_is_exact(&build)
        || build.lifecycle() != DraftPieceBuildLifecycleV1::Open
        || !writer_progress_is_current(acquisition.storage, build.writer_admission())
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let receipt = Box::new(required::<DraftPieceBuildProgressFamily>(
        acquisition,
        build.progress_receipt().key(),
    )?);
    if !progress_receipt_matches_build(&receipt, &build) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    receipt_effects(&acquisition, &receipt)?;
    let previous_ref = receipt
        .previous()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let previous = Box::new(required::<DraftPieceBuildProgressFamily>(
        acquisition,
        previous_ref.key(),
    )?);
    if previous.reference() != previous_ref {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    receipt_effects(&acquisition, &previous)?;
    let transition_fragment_key =
        super::mapping_custody::transition_fragment_key(&previous, &receipt);
    let scanned = transition_fragment_key
        .map(|key| required::<DraftPieceBuildFragmentsFamily>(&acquisition, key))
        .transpose()?;
    if !marker_effect_progress_transition_is_exact(&previous, &receipt, scanned.as_ref()) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    validate_previous_proof(&acquisition, &previous, &receipt)?;
    Ok(receipt)
}

#[inline(never)]
fn prepare_transition(
    acquisition: &BuildAcquisition<'_>,
    expected_revision: DomainRevision,
    build: Box<DraftPieceBuildRecordV1>,
    receipt: Box<DraftPieceBuildProgressReceiptV1>,
) -> Result<PreparedDraftPieceAdvanceV1, DraftPiecePrepareErrorV1> {
    let session_key =
        DraftEditorCandidateSessionRecordKeyV1::head(build.draft_id(), build.session_id());
    let DraftEditorCandidateSessionRecordV1::Head(session) =
        required::<DraftEditorCandidateSessionsFamily>(&acquisition, session_key)?
    else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    let open_key = DraftEditorCandidateSessionRecordKeyV1::open_receipt(
        build.draft_id(),
        build.session_id(),
        session.open_operation_id(),
    );
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(open) =
        required::<DraftEditorCandidateSessionsFamily>(&acquisition, open_key)?
    else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    if !super::super::session::receipt_matches_head(&open, &session)
        || session.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
        || session.active_operation() != Some(&custody_for(&build))
        || !active_session_generation_matches_build(&session, &build)
        || session.newest_root() != build.predecessor_root()
        || session.newest_history() != build.predecessor_history()
        || session.newest_candidate_generation() != build.predecessor_candidate_generation()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let staging = staging_fence(&acquisition, &build)?;
    let writer = build
        .writer_admission()
        .map(|admission| {
            let head = required::<DraftMarkerAdmissionHeadsFamily>(
                &acquisition,
                admission.binding().owner(),
            )?;
            if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Building
                || head.home_generation() != admission.binding().home_generation()
                || head.owner() != admission.binding().owner()
                || head.occurrence_commitment() != admission.binding().occurrence_commitment()
                || head.target_root() != admission.target_root()
                || head.remaining_builder_count() != admission.remaining_count()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            Ok(head)
        })
        .transpose()?;
    let fragment_ordinal = match build.frontier() {
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal, ..
        }
        | DraftPieceBuildFrontierV1::Planning { fragment_ordinal }
        | DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal, ..
        }
        | DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal, ..
        } => Some(fragment_ordinal),
        DraftPieceBuildFrontierV1::CrossValidating => None,
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    };
    let selected_fragment = fragment_ordinal
        .map(|ordinal| fragment(&acquisition, &build, ordinal))
        .transpose()?;
    let endpoint_fragment = (build.staged_fragment_count() != 0)
        .then(|| fragment(&acquisition, &build, build.staged_fragment_count()))
        .transpose()?;
    let quantum =
        advance_acquired_tree_build(acquisition.clone(), &build, selected_fragment.as_ref())?;
    let marker_continuation = quantum
        .marker_effect_continuation
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let mapping = quantum
        .mapping
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let writer_consumption = selected_fragment
        .as_ref()
        .map(|fragment| {
            super::marker_advance::prepare_consumption(
                &acquisition,
                &build,
                fragment,
                marker_continuation,
                writer.as_ref(),
            )
        })
        .transpose()?
        .flatten();
    let writer_admission = writer_consumption
        .as_ref()
        .map(|consumption| consumption.admission())
        .or(build.writer_admission());
    let (next, next_receipt) = next_build_record(
        &build,
        quantum.roots,
        quantum.base_frontier,
        quantum.successor_frontier,
        quantum.next_record_ordinal,
        quantum.frontier,
        quantum
            .successor
            .as_ref()
            .map(DraftPieceRootRecordV1::reference),
        quantum.build_digest,
        if quantum.frontier == DraftPieceBuildFrontierV1::Complete {
            DraftPieceBuildLifecycleV1::Complete
        } else {
            DraftPieceBuildLifecycleV1::Open
        },
        endpoint_fragment.as_ref().map(canonical_fragment_endpoint),
        marker_continuation,
        Some(mapping),
        writer_admission,
    )
    .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    if !marker_effect_progress_transition_is_exact(
        &receipt,
        &next_receipt,
        selected_fragment.as_ref(),
    ) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    validate_previous_proof(&acquisition, &receipt, &next_receipt)?;
    let next_session = session
        .advance_active_operation(&custody_for(&build), custody_for(&next))
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    Ok(PreparedDraftPieceAdvanceV1 {
        expected_revision,
        home_generation: acquisition.storage.home_generation,
        expected: build,
        expected_session: session,
        next: Box::new(next),
        next_receipt: Box::new(next_receipt),
        next_session,
        leaves: quantum.leaves,
        nodes: quantum.nodes,
        index_records: quantum.index_records,
        marker_order_records: quantum.marker_order_records,
        mapping_records: quantum.mapping_records,
        records_read: quantum.records_read,
        admission_marker: None,
        admission_consumption: writer_consumption
            .as_ref()
            .map(|value| value.index().clone()),
        bounded: Some(Box::new(SourceFences {
            budget: acquisition.budget.clone(),
            receipt,
            staging,
            writer,
            writer_consumption,
        })),
    })
}

#[inline(never)]
fn validate_target_records(
    acquisition: &BuildAcquisition<'_>,
    prepared: &PreparedDraftPieceAdvanceV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let key = DraftPieceSettlementKeyV1::new(
        prepared.expected.draft_id(),
        prepared.expected.session_id(),
        prepared.expected.operation_id(),
    );
    if acquisition
        .point::<DraftPieceSettlementsFamily>(key)?
        .is_some()
        || acquisition
            .point::<DraftPieceBuildProgressFamily>(prepared.next_receipt.key())?
            .is_some()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    for value in &prepared.leaves {
        if acquisition
            .point::<DraftPieceLeavesFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    for value in &prepared.nodes {
        if acquisition
            .point::<DraftPieceNodesFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    for value in &prepared.index_records {
        if acquisition
            .point::<DraftMarkerIdentityIndexFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    for value in &prepared.marker_order_records {
        if acquisition
            .point::<DraftMarkerOrderCommitmentsFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    acquisition
        .budget
        .emission::<DraftPieceBuildsFamily>(&key, prepared.next.as_ref())?;
    acquisition
        .budget
        .emission::<DraftPieceBuildProgressFamily>(
            &prepared.next_receipt.key(),
            prepared.next_receipt.as_ref(),
        )?;
    let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
        prepared.expected.draft_id(),
        prepared.expected.session_id(),
    );
    acquisition
        .budget
        .emission::<DraftEditorCandidateSessionsFamily>(
            &session_key,
            &DraftEditorCandidateSessionRecordV1::Head(prepared.next_session.clone()),
        )?;
    if acquisition
        .storage
        .revision(acquisition.store)
        .map_err(crate::SyndicReadError::from)?
        != prepared.expected_revision
        || acquisition.store.health().generation() != Some(acquisition.storage.home_generation)
    {
        return Err(DraftPiecePrepareErrorV1::ConcurrentChange);
    }
    Ok(())
}
