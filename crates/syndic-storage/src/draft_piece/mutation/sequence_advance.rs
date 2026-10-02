use super::advance_budget::{BuildAcquisition, BuildBudget};
use super::*;
use crate::draft_piece::staging::{
    draft_mutation_staging_head_is_locally_exact, draft_mutation_staging_receipt_is_locally_exact,
};

#[derive(Clone)]
pub(super) struct SourceFences {
    pub(super) budget: BuildBudget,
    pub(super) receipt: Box<DraftPieceBuildProgressReceiptV1>,
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
    super::mapping_custody::validate_roots_with(&mut acquisition.clone(), key, receipt.mapping())?;
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

mod preparation;

pub(super) use preparation::prepare;

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
        != Some(prepared.expected.as_ref())
        || source
            .budget
            .recheck::<DraftPieceBuildProgressFamily>(reader, &source.receipt.key())?
            .as_ref()
            != Some(source.receipt.as_ref())
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
