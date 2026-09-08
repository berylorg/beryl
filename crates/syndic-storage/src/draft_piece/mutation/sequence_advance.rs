use super::advance_budget::{BuildAcquisition, BuildBudget};
use super::*;
use crate::draft_piece::staging::{
    draft_mutation_staging_head_is_locally_exact, draft_mutation_staging_receipt_is_locally_exact,
};

#[derive(Clone)]
pub(super) struct SourceFences {
    pub(super) budget: BuildBudget,
    pub(super) receipt: DraftPieceBuildProgressReceiptV1,
    pub(super) staging: Option<DraftMutationStagingHeadV1>,
    writer: Option<DraftMarkerAdmissionHeadV1>,
    writer_consumption: Option<PreparedDraftMarkerWriterConsumptionV1>,
}

pub(super) fn required<F: crate::codec::Family>(
    acquisition: &BuildAcquisition<'_>,
    key: F::Key,
) -> Result<F::Value, DraftPiecePrepareErrorV1> {
    acquisition
        .point::<F>(key)?
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)
}

fn fragment(
    acquisition: &BuildAcquisition<'_>,
    build: &DraftPieceBuildRecordV1,
    ordinal: u64,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    if ordinal == 0 || ordinal > build.fragment_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let key = DraftPieceBuildFragmentKeyV1::new(
        build.draft_id(),
        build.session_id(),
        build.operation_id(),
        ordinal,
    );
    let value = required::<DraftPieceBuildFragmentsFamily>(acquisition, key)?;
    if value.key() != key
        || validate_fragment(value.replacement()).is_err()
        || value.chain_digest()
            != draft_piece_fragment_chain_link_v1(
                value.preceding_chain(),
                ordinal,
                value.replacement(),
            )
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if ordinal == 1 {
        if value.preceding_chain() != canonical_empty_draft_piece_fragment_chain_v1() {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    } else {
        let previous_key = DraftPieceBuildFragmentKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
            ordinal - 1,
        );
        let previous = required::<DraftPieceBuildFragmentsFamily>(acquisition, previous_key)?;
        if previous.key() != previous_key
            || validate_fragment(previous.replacement()).is_err()
            || previous.chain_digest() != value.preceding_chain()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    if ordinal == build.fragment_count() && value.chain_digest() != build.fragment_chain() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(value)
}

fn receipt_effects(
    acquisition: &BuildAcquisition<'_>,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    if !progress_receipt_is_exact(receipt) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if let Some(endpoint) = receipt.fragment_endpoint() {
        let value = required::<DraftPieceBuildFragmentsFamily>(acquisition, endpoint.key())?;
        if canonical_fragment_endpoint(&value) != endpoint {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    let key = DraftPieceSettlementKeyV1::new(
        receipt.key().draft_id(),
        receipt.key().session_id(),
        receipt.key().operation_id(),
    );
    validate_acquired_build_roots(acquisition, key, receipt.working_roots())?;
    if let Some(active) = receipt.marker_effect_continuation().active() {
        let value = required::<DraftPieceBuildFragmentsFamily>(acquisition, active.fragment_key())?;
        if canonical_fragment_endpoint(&value).digest() != active.fragment_digest()
            || value.replacement().marker_effect() != Some(active.effect())
            || active.source_roots() != receipt.working_roots()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        validate_acquired_build_roots(acquisition, key, active.working_roots())?;
        super::marker_advance::validate_pending_roots(acquisition, key.draft_id(), active)?;
    }
    Ok(())
}

fn staging_fence(
    acquisition: &BuildAcquisition<'_>,
    build: &DraftPieceBuildRecordV1,
) -> Result<Option<DraftMutationStagingHeadV1>, DraftPiecePrepareErrorV1> {
    let Some(continuation) = build.durable_continuation() else {
        return Ok(None);
    };
    let finished = continuation.finished();
    let head = required::<DraftMutationStagingHeadsFamily>(acquisition, finished.identity())?;
    let DraftMutationStagingLifecycleV1::Building(endpoint) = head.lifecycle() else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    if !draft_mutation_staging_head_is_locally_exact(&head)
        || head.identity() != finished.identity()
        || head.source() != finished.source()
        || head.proposal() != finished.proposal()
        || endpoint.key().draft_id() != build.draft_id()
        || endpoint.key().session_id() != build.session_id()
        || endpoint.key().operation_id() != build.operation_id()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let selected_key = DraftMutationStagingProgressReceiptKeyV1::new(
        head.identity(),
        head.receipt().transition_ordinal(),
    )
    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let selected = required::<DraftMutationStagingProgressFamily>(acquisition, selected_key)?;
    let prior_key = DraftMutationStagingProgressReceiptKeyV1::new(
        finished.identity(),
        finished.receipt().transition_ordinal(),
    )
    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let prior = required::<DraftMutationStagingProgressFamily>(acquisition, prior_key)?;
    if !draft_mutation_staging_receipt_is_locally_exact(&selected)
        || !draft_mutation_staging_receipt_is_locally_exact(&prior)
        || !super::super::staging::staging_finish_receipt_is_exact(&selected)
        || !super::super::staging::staging_finish_receipt_is_exact(&prior)
        || selected.key() != selected_key
        || prior.key() != prior_key
        || selected.digest() != head.receipt().digest()
        || selected.prior() != Some(finished.receipt())
        || selected.before_head_digest() != Some(finished.head_digest())
        || prior.digest() != finished.receipt().digest()
        || prior.after_head_digest() != finished.head_digest()
        || selected.after_head_digest() != head.digest()
        || selected.after_source() != head.source()
        || selected.after_proposal() != head.proposal()
        || selected.after_lifecycle() != head.lifecycle()
        || selected.command() != DraftMutationStagingCommandKindV1::Transfer
        || prior.command() != DraftMutationStagingCommandKindV1::Finish
        || selected.page().is_some()
        || prior.page().is_some()
        || selected.build_endpoint() != Some(endpoint)
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(Some(head))
}

pub(super) fn prepare(
    acquisition: BuildAcquisition<'_>,
    expected_revision: DomainRevision,
    build: DraftPieceBuildRecordV1,
) -> Result<PreparedDraftPieceAdvanceV1, DraftPiecePrepareErrorV1> {
    if !build_record_is_exact(&build)
        || build.lifecycle() != DraftPieceBuildLifecycleV1::Open
        || !writer_progress_is_current(acquisition.storage, build.writer_admission())
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let receipt =
        required::<DraftPieceBuildProgressFamily>(&acquisition, build.progress_receipt().key())?;
    if !progress_receipt_matches_build(&receipt, &build) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    receipt_effects(&acquisition, &receipt)?;
    let previous_ref = receipt
        .previous()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let previous = required::<DraftPieceBuildProgressFamily>(&acquisition, previous_ref.key())?;
    if previous.reference() != previous_ref {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    receipt_effects(&acquisition, &previous)?;
    let transition_fragment_key = receipt
        .marker_effect_continuation()
        .active()
        .or(previous.marker_effect_continuation().active())
        .map(|active| active.fragment_key())
        .or_else(|| {
            receipt
                .marker_effect_continuation()
                .scan()
                .scanned_endpoint()
                .map(|endpoint| endpoint.key())
        });
    let scanned = transition_fragment_key
        .map(|key| required::<DraftPieceBuildFragmentsFamily>(&acquisition, key))
        .transpose()?;
    if !marker_effect_progress_transition_is_exact(&previous, &receipt, scanned.as_ref()) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    validate_previous_proof(&acquisition, &previous, &receipt)?;
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
        } => fragment_ordinal,
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    };
    let selected_fragment = fragment(&acquisition, &build, fragment_ordinal)?;
    let endpoint_fragment = fragment(&acquisition, &build, build.staged_fragment_count())?;
    let quantum = advance_acquired_tree_build(acquisition.clone(), &build, &selected_fragment)?;
    let (roots, marker_continuation) = match quantum.marker_effect_continuation {
        Some(continuation) => (quantum.roots, continuation),
        None => marker_continuation_transition(
            &build,
            Some(&selected_fragment),
            quantum.roots,
            quantum.frontier,
        )?,
    };
    let writer_consumption = super::marker_advance::prepare_consumption(
        &acquisition,
        &build,
        &selected_fragment,
        marker_continuation,
        writer.as_ref(),
    )?;
    let writer_admission = writer_consumption
        .as_ref()
        .map(|consumption| consumption.admission())
        .or(build.writer_admission());
    let (next, next_receipt) = next_build_record(
        &build,
        roots,
        quantum.base_frontier,
        quantum.successor_frontier,
        quantum.next_record_ordinal,
        quantum.frontier,
        None,
        None,
        DraftPieceBuildLifecycleV1::Open,
        Some(canonical_fragment_endpoint(&endpoint_fragment)),
        marker_continuation,
        writer_admission,
    )
    .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    if !marker_effect_progress_transition_is_exact(
        &receipt,
        &next_receipt,
        Some(&selected_fragment),
    ) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    validate_previous_proof(&acquisition, &receipt, &next_receipt)?;
    let next_session = session
        .advance_active_operation(&custody_for(&build), custody_for(&next))
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let key =
        DraftPieceSettlementKeyV1::new(build.draft_id(), build.session_id(), build.operation_id());
    if acquisition
        .point::<DraftPieceSettlementsFamily>(key)?
        .is_some()
        || acquisition
            .point::<DraftPieceBuildProgressFamily>(next_receipt.key())?
            .is_some()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    for value in &quantum.leaves {
        if acquisition
            .point::<DraftPieceLeavesFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    for value in &quantum.nodes {
        if acquisition
            .point::<DraftPieceNodesFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    for value in &quantum.index_records {
        if acquisition
            .point::<DraftMarkerIdentityIndexFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    for value in &quantum.marker_order_records {
        if acquisition
            .point::<DraftMarkerOrderCommitmentsFamily>(value.key())?
            .is_some()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    acquisition
        .budget
        .emission::<DraftPieceBuildsFamily>(&key, &next)?;
    acquisition
        .budget
        .emission::<DraftPieceBuildProgressFamily>(&next_receipt.key(), &next_receipt)?;
    acquisition
        .budget
        .emission::<DraftEditorCandidateSessionsFamily>(
            &session_key,
            &DraftEditorCandidateSessionRecordV1::Head(next_session.clone()),
        )?;
    if acquisition
        .storage
        .revision(acquisition.store)
        .map_err(crate::SyndicReadError::from)?
        != expected_revision
        || acquisition.store.health().generation() != Some(acquisition.storage.home_generation)
    {
        return Err(DraftPiecePrepareErrorV1::ConcurrentChange);
    }
    Ok(PreparedDraftPieceAdvanceV1 {
        expected_revision,
        home_generation: acquisition.storage.home_generation,
        expected: build,
        expected_session: session,
        next,
        next_receipt,
        next_session,
        leaves: quantum.leaves,
        nodes: quantum.nodes,
        index_records: quantum.index_records,
        marker_order_records: quantum.marker_order_records,
        records_read: quantum.records_read,
        admission_marker: None,
        admission_consumption: writer_consumption
            .as_ref()
            .map(|value| value.index().clone()),
        bounded: Some(SourceFences {
            budget: acquisition.budget,
            receipt,
            staging,
            writer,
            writer_consumption,
        }),
    })
}

fn validate_previous_proof(
    acquisition: &BuildAcquisition<'_>,
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let fragment = super::marker_advance::previous_proof_fragment_key(previous, current)
        .map(|key| required::<DraftPieceBuildFragmentsFamily>(acquisition, key))
        .transpose()?;
    if !super::marker_advance::previous_proof_transition_is_exact(
        previous,
        current,
        fragment.as_ref(),
    ) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(())
}

pub(super) fn submit(
    prepared: PreparedDraftPieceAdvanceV1,
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<
    Option<(
        PreparedDraftPieceAdvanceV1,
        Option<PreparedDraftMarkerWriterConsumptionV1>,
    )>,
    SyndicMutationError,
> {
    let source = prepared
        .bounded
        .as_ref()
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let key = DraftPieceSettlementKeyV1::new(
        prepared.expected.draft_id(),
        prepared.expected.session_id(),
        prepared.expected.operation_id(),
    );
    if source
        .budget
        .recheck::<DraftPieceBuildsFamily>(reader, &key)?
        .as_ref()
        != Some(&prepared.expected)
        || source
            .budget
            .recheck::<DraftPieceBuildProgressFamily>(reader, &source.receipt.key())?
            .as_ref()
            != Some(&source.receipt)
        || source
            .budget
            .recheck::<DraftEditorCandidateSessionsFamily>(
                reader,
                &DraftEditorCandidateSessionRecordKeyV1::head(key.draft_id(), key.session_id()),
            )?
            != Some(DraftEditorCandidateSessionRecordV1::Head(
                prepared.expected_session.clone(),
            ))
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if let Some(staging) = &source.staging {
        if source
            .budget
            .recheck::<DraftMutationStagingHeadsFamily>(reader, &staging.identity())?
            .as_ref()
            != Some(staging)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    if let Some(writer) = &source.writer {
        if source
            .budget
            .recheck::<DraftMarkerAdmissionHeadsFamily>(reader, &writer.owner())?
            .as_ref()
            != Some(writer)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    if let Some(consumption) = &source.writer_consumption {
        if source
            .budget
            .recheck::<DraftMarkerAdmissionCapacityFamily>(
                reader,
                &DraftMarkerAdmissionCapacityKeyV1,
            )?
            .as_ref()
            != Some(consumption.prior_capacity())
            || source.writer.as_ref() != Some(consumption.prior_head())
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    let writer_consumption = source.writer_consumption.clone();
    Ok(Some((prepared, writer_consumption)))
}
