use super::*;

fn assignment_point<F: crate::codec::Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    work: &AdmissionWorkLedger,
    key: &F::Key,
) -> Result<Option<F::Value>, AssignmentMutationError> {
    work.point::<F, AssignmentMutationError>(
        key,
        AdmissionWorkLedger::family_maximum::<F>(),
        || {
            reader
                .point::<crate::codec::ExactCodec<F>>(
                    key,
                    beryl_home_store::PointReadLimit::new(
                        AdmissionWorkLedger::family_value_limit::<F>(),
                    )
                    .expect("admission control point bound is nonzero"),
                )
                .map_err(Into::into)
        },
    )
}
use crate::DraftMarkerAdmissionLimitsV1;

pub(super) struct AssignmentMutation {
    pub(super) owner: DraftMarkerAdmissionOwnerV1,
    pub(super) command: DraftMarkerAdmissionCommandIdV1,
    pub(super) authority: DraftMarkerAdmissionLiveAuthorityV1,
    pub(super) retained_limits: DraftMarkerAdmissionLimitsV1,
    pub(super) work: AdmissionWorkLedger,
}

pub(super) struct PreparedAssignmentMutation {
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    receipt: DraftMarkerAdmissionReplayReceiptV1,
    prior_receipt_key: DraftMarkerAdmissionReceiptKeyV1,
    index: PreparedDraftMarkerAdmissionIndexSuccessorV1,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum AssignmentMutationError {
    #[error(transparent)]
    Limit(#[from] super::super::refusal::AdmissionLimitError),
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Schema(#[from] DraftMarkerAdmissionSchemaErrorV1),
    #[error("draft-marker assignment index failed: {0}")]
    Index(DraftMarkerAdmissionIndexPreparationErrorV1),
    #[error("draft-marker assignment authority disagrees")]
    Authority,
    #[error("draft-marker assignment command collides")]
    Collision,
    #[error("draft-marker assignment charge disagrees")]
    Charge,
}

impl From<DraftMarkerAdmissionIndexPreparationErrorV1> for AssignmentMutationError {
    fn from(value: DraftMarkerAdmissionIndexPreparationErrorV1) -> Self {
        match value {
            DraftMarkerAdmissionIndexPreparationErrorV1::Schema(error) => Self::Schema(error),
            DraftMarkerAdmissionIndexPreparationErrorV1::Read(error) => Self::Read(error),
            DraftMarkerAdmissionIndexPreparationErrorV1::OperationTooLarge => {
                Self::Limit(super::super::refusal::AdmissionLimitError::OperationTooLarge)
            }
            other => Self::Index(other),
        }
    }
}

impl DomainCallbackError for AssignmentMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

impl DomainMutation<SyndicDomain> for AssignmentMutation {
    type Error = AssignmentMutationError;
    type Prepared = PreparedAssignmentMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        prepare_assignment(reader, self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(2)?;
        reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(
            usize::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 6 + 4,
        )?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<DraftMarkerAdmissionReceiptsCodec>(&prepared.prior_receipt_key)?;
        for node in prepared.index.deletions() {
            mutations.delete::<DraftMarkerAdmissionNodesCodec>(&node.key())?;
        }
        for node in prepared.index.puts() {
            mutations.put::<DraftMarkerAdmissionNodesCodec>(&node.key(), node)?;
        }
        mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
        mutations.put::<DraftMarkerAdmissionReceiptsCodec>(
            &DraftMarkerAdmissionReceiptKeyV1::new(
                prepared.receipt.owner(),
                prepared.receipt.command_id(),
            ),
            &prepared.receipt,
        )?;
        mutations.put::<DraftMarkerAdmissionCapacityCodec>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.capacity,
        )?;
        Ok(())
    }
}

fn prepare_assignment(
    reader: &DomainReader<'_, SyndicDomain>,
    mutation: AssignmentMutation,
) -> Result<PreparedAssignmentMutation, AssignmentMutationError> {
    request_authority_exact_read_bytes_with_ledger(
        reader,
        &mutation.authority.authority,
        &mutation.work,
    )?
    .ok_or(AssignmentMutationError::Authority)?;
    let capacity = assignment_point::<DraftMarkerAdmissionCapacityFamily>(
        reader,
        &mutation.work,
        &DraftMarkerAdmissionCapacityKeyV1,
    )?
    .ok_or(AssignmentMutationError::Authority)?;
    let head = assignment_point::<DraftMarkerAdmissionHeadsFamily>(
        reader,
        &mutation.work,
        &mutation.owner,
    )?
    .ok_or(AssignmentMutationError::Authority)?;
    if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Assigning
        || head.home_generation() != mutation.authority.authority.home_generation
        || head.request_commitment() != mutation.authority.authority.request_commitment()
        || head.custody_commitment() != mutation.authority.authority.custody_commitment()
        || head
            .assignment_continuation()
            .and_then(|value| value.allocation_range())
            != mutation.authority.allocation_range
    {
        return Err(AssignmentMutationError::Authority);
    }
    let prior_command = head
        .selected_receipt()
        .ok_or(AssignmentMutationError::Authority)?;
    if prior_command == mutation.command {
        return Err(AssignmentMutationError::Collision);
    }
    if assignment_point::<DraftMarkerAdmissionReceiptsFamily>(
        reader,
        &mutation.work,
        &DraftMarkerAdmissionReceiptKeyV1::new(mutation.owner, mutation.command),
    )?
    .is_some()
    {
        return Err(AssignmentMutationError::Collision);
    }
    let prior_receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(mutation.owner, prior_command);
    let prior_receipt = assignment_point::<DraftMarkerAdmissionReceiptsFamily>(
        reader,
        &mutation.work,
        &prior_receipt_key,
    )?
    .ok_or(AssignmentMutationError::Authority)?;
    if prior_receipt.owner() != mutation.owner
        || prior_receipt.command_id() != prior_command
        || prior_receipt.request_commitment() != head.request_commitment()
        || prior_receipt.source_after() != head.source_root()
        || prior_receipt.target_after() != head.target_root()
    {
        return Err(AssignmentMutationError::Authority);
    }
    let processed = head
        .target_root()
        .count()
        .checked_sub(head.source_root().count())
        .ok_or(AssignmentMutationError::Authority)?;
    let continuation = head
        .assignment_continuation()
        .ok_or(AssignmentMutationError::Authority)?;
    if head.source_root().count() == 0 {
        if head.target_root().count() != 0
            || continuation != DraftMarkerAdmissionAssignmentContinuationV1::reuse(None)
        {
            return Err(AssignmentMutationError::Authority);
        }
        let index = prepare_empty_draft_marker_admission_assignment_v1(
            reader,
            head.owner(),
            head.source_root(),
            head.target_root(),
            &prior_receipt,
            &mutation.work,
        )?;
        let source_closure = empty_assignment_source_closure(&head);
        let target_closure = empty_assignment_target_closure(&head);
        return finish_assignment_transition(
            capacity,
            head,
            prior_receipt,
            prior_receipt_key,
            mutation.command,
            index,
            continuation,
            source_closure,
            target_closure,
            mutation.retained_limits,
            &mutation.work,
        );
    }
    let assignment = prepare_draft_marker_admission_assignment_v1(
        reader,
        mutation.owner,
        head.source_root(),
        head.target_root(),
        &prior_receipt,
        mutation.command,
        processed,
        continuation,
        &mutation.work,
    )?;
    finish_prepared_assignment(
        reader,
        capacity,
        head,
        prior_receipt,
        prior_receipt_key,
        mutation.command,
        assignment,
        mutation.retained_limits,
        &mutation.work,
    )
}

fn finish_prepared_assignment(
    _reader: &DomainReader<'_, SyndicDomain>,
    capacity: DraftMarkerAdmissionCapacityV1,
    prior_head: DraftMarkerAdmissionHeadV1,
    prior_receipt: DraftMarkerAdmissionReplayReceiptV1,
    prior_receipt_key: DraftMarkerAdmissionReceiptKeyV1,
    command: DraftMarkerAdmissionCommandIdV1,
    assignment: PreparedDraftMarkerAdmissionAssignmentV1,
    retained_limits: DraftMarkerAdmissionLimitsV1,
    work: &AdmissionWorkLedger,
) -> Result<PreparedAssignmentMutation, AssignmentMutationError> {
    let source_closure =
        assignment_source_closure(&prior_head, assignment.group, assignment.asset_id);
    let target_closure = assignment_target_closure(&prior_head, assignment.assigned_label);
    finish_assignment_transition(
        capacity,
        prior_head,
        prior_receipt,
        prior_receipt_key,
        command,
        assignment.index,
        assignment.continuation,
        source_closure,
        target_closure,
        retained_limits,
        work,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_assignment_transition(
    capacity: DraftMarkerAdmissionCapacityV1,
    prior_head: DraftMarkerAdmissionHeadV1,
    prior_receipt: DraftMarkerAdmissionReplayReceiptV1,
    prior_receipt_key: DraftMarkerAdmissionReceiptKeyV1,
    command: DraftMarkerAdmissionCommandIdV1,
    index: PreparedDraftMarkerAdmissionIndexSuccessorV1,
    continuation: DraftMarkerAdmissionAssignmentContinuationV1,
    source_closure: Box<[u8]>,
    target_closure: Box<[u8]>,
    retained_limits: DraftMarkerAdmissionLimitsV1,
    work: &AdmissionWorkLedger,
) -> Result<PreparedAssignmentMutation, AssignmentMutationError> {
    let ready = index.source_root().count() == 0;
    let next_revision = NonZeroU64::new(
        prior_head
            .revision()
            .get()
            .checked_add(1)
            .ok_or(AssignmentMutationError::Authority)?,
    )
    .ok_or(AssignmentMutationError::Authority)?;
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        prior_head.owner(),
        command,
        prior_head.next_page_ordinal(),
        prior_head.request_commitment(),
        source_closure,
        target_closure,
        prior_head.source_root(),
        index.source_root(),
        prior_head.target_root(),
        index.target_root(),
        index.retained_predecessor_nodes(),
        DraftMarkerAdmissionReceiptTransitionV1::Assignment,
    )?;
    let provisional = assignment_head(
        &prior_head,
        next_revision,
        &index,
        ready,
        continuation,
        DraftMarkerAdmissionRetainedChargeV1::new(1, index.target_root().count(), 0),
        command,
    )?;
    let successor_receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(prior_head.owner(), command);
    let successor_metadata = encoded_head_record_charge(&prior_head.owner(), &provisional)?
        .checked_add(encoded_receipt_record_charge(
            &successor_receipt_key,
            &receipt,
        )?)
        .ok_or(AssignmentMutationError::Charge)?;
    let prior_metadata = encoded_head_record_charge(&prior_head.owner(), &prior_head)?
        .checked_add(encoded_receipt_record_charge(
            &prior_receipt_key,
            &prior_receipt,
        )?)
        .ok_or(AssignmentMutationError::Charge)?;
    let delta = index.retained_charge_delta();
    let successor_charge = prior_head
        .charge()
        .checked_sub(DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            0,
            prior_metadata,
        ))
        .and_then(|charge| charge.checked_add(delta.added()))
        .and_then(|charge| charge.checked_sub(delta.removed()))
        .and_then(|charge| {
            charge.checked_add(DraftMarkerAdmissionRetainedChargeV1::new(
                0,
                0,
                successor_metadata,
            ))
        })
        .ok_or(AssignmentMutationError::Charge)?;
    super::super::refusal::check_operation_charge(successor_charge, retained_limits)?;
    let head = assignment_head(
        &prior_head,
        next_revision,
        &index,
        ready,
        continuation,
        successor_charge,
        command,
    )?;
    let aggregate = capacity
        .charge()
        .checked_sub(prior_head.charge())
        .and_then(|charge| charge.checked_add(successor_charge))
        .ok_or(AssignmentMutationError::Charge)?;
    super::super::refusal::check_aggregate_charge(aggregate, retained_limits)?;
    let capacity = DraftMarkerAdmissionCapacityV1::new(
        NonZeroU64::new(
            capacity
                .revision()
                .get()
                .checked_add(1)
                .ok_or(AssignmentMutationError::Charge)?,
        )
        .ok_or(AssignmentMutationError::Charge)?,
        aggregate,
    )?;
    work.charge_emit(
        encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &capacity)?,
        false,
    )?;
    work.charge_emit(encoded_head_record_charge(&head.owner(), &head)?, false)?;
    work.charge_emit(
        encoded_receipt_record_charge(&successor_receipt_key, &receipt)?,
        false,
    )?;
    work.charge_delete(encoded_receipt_record_charge(
        &prior_receipt_key,
        &prior_receipt,
    )?)?;
    Ok(PreparedAssignmentMutation {
        capacity,
        head,
        receipt,
        prior_receipt_key,
        index,
    })
}

fn assignment_head(
    prior: &DraftMarkerAdmissionHeadV1,
    revision: NonZeroU64,
    index: &PreparedDraftMarkerAdmissionIndexSuccessorV1,
    ready: bool,
    continuation: DraftMarkerAdmissionAssignmentContinuationV1,
    charge: DraftMarkerAdmissionRetainedChargeV1,
    command: DraftMarkerAdmissionCommandIdV1,
) -> Result<DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionHeadV1::new(
        prior.owner(),
        revision,
        prior.home_generation(),
        if ready {
            DraftMarkerAdmissionLifecycleV1::Ready
        } else {
            DraftMarkerAdmissionLifecycleV1::Assigning
        },
        prior.request_commitment(),
        prior.custody_commitment(),
        prior.next_page_ordinal(),
        0,
        true,
        Some(command),
        index.source_root(),
        index.target_root(),
        prior.occurrence_commitment(),
        prior.allocating_occurrence_count(),
        prior.occurrence_count(),
        index.source_root().count(),
        (!ready).then_some(continuation),
        if ready {
            index.target_root().count()
        } else {
            0
        },
        charge,
        None,
    )
}

fn assignment_source_closure(
    head: &DraftMarkerAdmissionHeadV1,
    group: crate::draft_piece::DraftMarkerAdmissionAssignmentGroupV1,
    asset: AssetId,
) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(120);
    bytes.extend_from_slice(head.digest().as_bytes());
    bytes.extend_from_slice(&group.canonical_bytes());
    bytes.extend_from_slice(&asset.digest());
    bytes.extend_from_slice(&asset.length().get().to_le_bytes());
    bytes.into_boxed_slice()
}

fn empty_assignment_source_closure(head: &DraftMarkerAdmissionHeadV1) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(72);
    bytes.extend_from_slice(b"syndic/draft-marker-empty-assignment-source/v1");
    bytes.extend_from_slice(head.digest().as_bytes());
    bytes.into_boxed_slice()
}

fn assignment_target_closure(
    head: &DraftMarkerAdmissionHeadV1,
    label: ImageLabelOrdinal,
) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(80);
    bytes.extend_from_slice(head.target_root().digest().as_bytes());
    bytes.extend_from_slice(&head.target_root().count().to_le_bytes());
    bytes.extend_from_slice(&label.get().to_le_bytes());
    bytes.into_boxed_slice()
}

fn empty_assignment_target_closure(head: &DraftMarkerAdmissionHeadV1) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(72);
    bytes.extend_from_slice(b"syndic/draft-marker-empty-assignment-target/v1");
    bytes.extend_from_slice(head.digest().as_bytes());
    bytes.into_boxed_slice()
}

#[derive(Clone, Copy)]
pub(super) enum AssignmentFailureClass {
    Collision,
    Rejected,
    Unavailable,
    OperationTooLarge,
    CapacityUnavailable,
    StorageError,
    Retryable,
}

pub(super) fn classify_assignment_failure(error: &CommandError) -> AssignmentFailureClass {
    if super::super::refusal::is_storage_command_error(error) {
        return AssignmentFailureClass::StorageError;
    }
    if matches!(error, CommandError::ReconciliationCapacity) {
        return AssignmentFailureClass::CapacityUnavailable;
    }
    let source = match error {
        CommandError::ContributorValidation { source, .. }
        | CommandError::ContributorReservation { source, .. }
        | CommandError::ContributorAssembly { source, .. } => Some(source.as_ref()),
        _ => None,
    };
    match source.and_then(|source| source.downcast_ref::<AssignmentMutationError>()) {
        Some(AssignmentMutationError::Limit(
            super::super::refusal::AdmissionLimitError::OperationTooLarge,
        )) => AssignmentFailureClass::OperationTooLarge,
        Some(AssignmentMutationError::Limit(
            super::super::refusal::AdmissionLimitError::CapacityUnavailable,
        )) => AssignmentFailureClass::CapacityUnavailable,
        Some(AssignmentMutationError::Collision) => AssignmentFailureClass::Collision,
        Some(
            AssignmentMutationError::Authority
            | AssignmentMutationError::Index(
                DraftMarkerAdmissionIndexPreparationErrorV1::SourceTargetDisagreement,
            )
            | AssignmentMutationError::Schema(DraftMarkerAdmissionSchemaErrorV1::InvalidHead),
        ) => AssignmentFailureClass::Rejected,
        Some(AssignmentMutationError::Schema(
            DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge,
        )) => AssignmentFailureClass::Unavailable,
        Some(_) => AssignmentFailureClass::Rejected,
        _ => AssignmentFailureClass::Retryable,
    }
}
