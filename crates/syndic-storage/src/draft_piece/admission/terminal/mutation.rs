use std::num::NonZeroU64;

use beryl_home_store::{
    CommandError, CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError,
    DomainCallbackSource, DomainMutation, DomainReader, MutationBuildError, MutationBuilder,
    ReadError, ReconciliationReservation,
};

use crate::{
    codec::family_point_limit,
    domain::SyndicDomain,
    draft_piece::{
        DraftMarkerAdmissionCapacityCodec, DraftMarkerAdmissionCapacityFamily,
        DraftMarkerAdmissionCapacityKeyV1, DraftMarkerAdmissionCapacityV1,
        DraftMarkerAdmissionChildV1, DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodeKeyV1,
        DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionNodesCodec,
        DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionReceiptKeyV1,
        DraftMarkerAdmissionReceiptTransitionV1, DraftMarkerAdmissionReceiptsCodec,
        DraftMarkerAdmissionReceiptsFamily, DraftMarkerAdmissionReplayReceiptV1,
        DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionSchemaErrorV1,
        DraftMarkerAdmissionTreeV1, canonical_empty_draft_marker_admission_root_v1,
        checked_draft_marker_admission_command_charge_v1, encoded_capacity_record_charge,
        encoded_head_record_charge, encoded_node_record_charge, encoded_receipt_record_charge,
    },
};

use super::super::DraftMarkerAdmissionCleanupCursorV1;
use super::closure::{
    TERMINAL_READ_BYTES, TerminalClosureError, node_first, node_last, read_terminal_closure,
    terminal_receipt_is_exact, terminal_source_closure, terminal_target_closure,
};

const CLEANUP_PAGE_ITEMS: usize = 64;

#[derive(Clone, Copy)]
pub(super) enum TerminalMutationMode {
    CancelCurrent(u64),
    Cleanup(u64),
}

pub(super) struct TerminalMutation {
    pub(super) owner: DraftMarkerAdmissionOwnerV1,
    pub(super) command: DraftMarkerAdmissionCommandIdV1,
    pub(super) mode: TerminalMutationMode,
}

pub(super) struct PreparedTerminalMutation {
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    receipt_put: Option<DraftMarkerAdmissionReplayReceiptV1>,
    receipt_delete: Option<DraftMarkerAdmissionReceiptKeyV1>,
    node_deletions: Box<[DraftMarkerAdmissionNodeKeyV1]>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum TerminalMutationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Schema(#[from] DraftMarkerAdmissionSchemaErrorV1),
    #[error("draft-marker terminal authority disagrees")]
    Authority,
    #[error("draft-marker terminal identity collides")]
    Collision,
    #[error("draft-marker terminal retained charge disagrees")]
    Charge,
    #[error("draft-marker terminal closure is already compact")]
    AlreadyCompact,
}

impl DomainCallbackError for TerminalMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

impl DomainMutation<SyndicDomain> for TerminalMutation {
    type Error = TerminalMutationError;
    type Prepared = PreparedTerminalMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let capacity = reader
            .point::<DraftMarkerAdmissionCapacityCodec>(
                &DraftMarkerAdmissionCapacityKeyV1,
                family_point_limit::<DraftMarkerAdmissionCapacityFamily>(),
            )?
            .ok_or(TerminalMutationError::Authority)?;
        let head = reader
            .point::<DraftMarkerAdmissionHeadsCodec>(
                &self.owner,
                family_point_limit::<DraftMarkerAdmissionHeadsFamily>(),
            )?
            .ok_or(TerminalMutationError::Authority)?;
        match head.lifecycle() {
            DraftMarkerAdmissionLifecycleV1::Ingesting
            | DraftMarkerAdmissionLifecycleV1::Assigning
            | DraftMarkerAdmissionLifecycleV1::Ready => {
                prepare_terminalization(reader, capacity, head, self)
            }
            DraftMarkerAdmissionLifecycleV1::TerminalCleanup => {
                if !matches!(self.mode, TerminalMutationMode::Cleanup(_)) {
                    return Err(TerminalMutationError::Collision);
                }
                prepare_cleanup(reader, capacity, head)
            }
            DraftMarkerAdmissionLifecycleV1::Staging
            | DraftMarkerAdmissionLifecycleV1::Building
            | DraftMarkerAdmissionLifecycleV1::Settled => Err(TerminalMutationError::Authority),
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(2)?;
        reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(CLEANUP_PAGE_ITEMS)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(key) = prepared.receipt_delete {
            mutations.delete::<DraftMarkerAdmissionReceiptsCodec>(&key)?;
        }
        for key in prepared.node_deletions.iter() {
            mutations.delete::<DraftMarkerAdmissionNodesCodec>(key)?;
        }
        mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
        if let Some(receipt) = prepared.receipt_put {
            mutations.put::<DraftMarkerAdmissionReceiptsCodec>(
                &DraftMarkerAdmissionReceiptKeyV1::new(receipt.owner(), receipt.command_id()),
                &receipt,
            )?;
        }
        mutations.put::<DraftMarkerAdmissionCapacityCodec>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.capacity,
        )?;
        Ok(())
    }
}

fn prepare_terminalization(
    reader: &DomainReader<'_, SyndicDomain>,
    capacity: DraftMarkerAdmissionCapacityV1,
    prior_head: DraftMarkerAdmissionHeadV1,
    mutation: TerminalMutation,
) -> Result<PreparedTerminalMutation, TerminalMutationError> {
    let current_generation = match mutation.mode {
        TerminalMutationMode::CancelCurrent(generation) => {
            if prior_head.home_generation().get() != generation {
                return Err(TerminalMutationError::Authority);
            }
            generation
        }
        TerminalMutationMode::Cleanup(generation) => {
            if prior_head.home_generation().get() == generation {
                return Err(TerminalMutationError::Authority);
            }
            generation
        }
    };
    if current_generation == 0 {
        return Err(TerminalMutationError::Authority);
    }
    let prior_command = prior_head
        .selected_receipt()
        .ok_or(TerminalMutationError::Authority)?;
    if prior_command == mutation.command {
        return Err(TerminalMutationError::Collision);
    }
    let prior_receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(mutation.owner, prior_command);
    let prior_receipt = reader
        .point::<DraftMarkerAdmissionReceiptsCodec>(
            &prior_receipt_key,
            family_point_limit::<DraftMarkerAdmissionReceiptsFamily>(),
        )?
        .ok_or(TerminalMutationError::Authority)?;
    if prior_receipt.owner() != mutation.owner
        || prior_receipt.command_id() != prior_command
        || prior_receipt.request_commitment() != prior_head.request_commitment()
        || prior_receipt.source_after() != prior_head.source_root()
        || prior_receipt.target_after() != prior_head.target_root()
        || prior_receipt.transition() == DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup
    {
        return Err(TerminalMutationError::Authority);
    }
    let source_empty =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let target_empty =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        mutation.owner,
        mutation.command,
        prior_head.next_page_ordinal(),
        prior_head.request_commitment(),
        terminal_source_closure(mutation.owner, mutation.command),
        terminal_target_closure(&prior_head),
        prior_head.source_root(),
        source_empty,
        prior_head.target_root(),
        target_empty,
        Box::<[DraftMarkerAdmissionChildV1]>::default(),
        DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup,
    )?;
    let next_revision = increment(prior_head.revision())?;
    let provisional = terminal_head(
        &prior_head,
        next_revision,
        DraftMarkerAdmissionRetainedChargeV1::new(1, prior_head.charge().associations(), 0),
        DraftMarkerAdmissionCleanupCursorV1::new(DraftMarkerAdmissionTreeV1::SourceOrder, None),
    )?;
    let old_metadata = encoded_head_record_charge(&mutation.owner, &prior_head)?
        .checked_add(encoded_receipt_record_charge(
            &prior_receipt_key,
            &prior_receipt,
        )?)
        .ok_or(TerminalMutationError::Charge)?;
    let receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(mutation.owner, mutation.command);
    let new_metadata = encoded_head_record_charge(&mutation.owner, &provisional)?
        .checked_add(encoded_receipt_record_charge(&receipt_key, &receipt)?)
        .ok_or(TerminalMutationError::Charge)?;
    let successor_charge = prior_head
        .charge()
        .checked_sub(DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            0,
            old_metadata,
        ))
        .and_then(|charge| {
            charge.checked_add(DraftMarkerAdmissionRetainedChargeV1::new(
                0,
                0,
                new_metadata,
            ))
        })
        .ok_or(TerminalMutationError::Charge)?;
    let head = terminal_head(
        &prior_head,
        next_revision,
        successor_charge,
        DraftMarkerAdmissionCleanupCursorV1::new(DraftMarkerAdmissionTreeV1::SourceOrder, None),
    )?;
    let capacity = exchange_capacity(capacity, prior_head.charge(), successor_charge)?;
    checked_draft_marker_admission_command_charge_v1([
        encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &capacity)?
            .checked_add(old_metadata)
            .ok_or(TerminalMutationError::Charge)?,
        encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &capacity)?
            .checked_add(encoded_head_record_charge(&mutation.owner, &head)?)
            .and_then(|bytes| {
                bytes.checked_add(encoded_receipt_record_charge(&receipt_key, &receipt).ok()?)
            })
            .ok_or(TerminalMutationError::Charge)?,
        encoded_receipt_record_charge(&prior_receipt_key, &prior_receipt)?,
    ])?;
    Ok(PreparedTerminalMutation {
        capacity,
        head,
        receipt_put: Some(receipt),
        receipt_delete: Some(prior_receipt_key),
        node_deletions: Box::new([]),
    })
}

fn prepare_cleanup(
    reader: &DomainReader<'_, SyndicDomain>,
    capacity: DraftMarkerAdmissionCapacityV1,
    prior_head: DraftMarkerAdmissionHeadV1,
) -> Result<PreparedTerminalMutation, TerminalMutationError> {
    let closure = read_terminal_closure(reader, &prior_head).map_err(|error| match error {
        TerminalClosureError::Read(error) => TerminalMutationError::Read(error),
        TerminalClosureError::Invalid => TerminalMutationError::Collision,
    })?;
    let receipt_key = closure.key;
    let receipt = closure.receipt;
    let receipt_read_bytes = closure.encoded_bytes;
    let cursor = prior_head
        .cleanup_cursor()
        .ok_or(TerminalMutationError::Authority)?;
    let page = reader.cursor::<DraftMarkerAdmissionNodesCodec>(
        &node_range(prior_head.owner(), cursor.after()),
        CursorDirection::Forward,
        CursorReadLimits::new(CLEANUP_PAGE_ITEMS, TERMINAL_READ_BYTES)
            .expect("draft-marker cleanup cursor limits are nonzero"),
    )?;
    let mut removed_bytes = 0_u64;
    let mut removed_associations = 0_u64;
    let mut deletions = Vec::with_capacity(page.records().len());
    for record in page.records() {
        if record.key().owner() != prior_head.owner() || record.value().key() != *record.key() {
            return Err(TerminalMutationError::Authority);
        }
        removed_bytes = removed_bytes
            .checked_add(encoded_node_record_charge(record.key(), record.value())?)
            .ok_or(TerminalMutationError::Charge)?;
        if matches!(
            record.value().payload(),
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf { .. }
        ) {
            removed_associations = removed_associations
                .checked_add(1)
                .ok_or(TerminalMutationError::Charge)?;
        }
        deletions.push(*record.key());
    }
    let next_cursor = if let Some(last) = deletions.last().copied() {
        DraftMarkerAdmissionCleanupCursorV1::new(cursor.tree(), Some(last))
    } else if cursor.tree() == DraftMarkerAdmissionTreeV1::SourceOrder {
        DraftMarkerAdmissionCleanupCursorV1::new(DraftMarkerAdmissionTreeV1::TargetId, None)
    } else {
        return Err(TerminalMutationError::AlreadyCompact);
    };
    let next_revision = increment(prior_head.revision())?;
    let old_head_bytes = encoded_head_record_charge(&prior_head.owner(), &prior_head)?;
    let provisional = terminal_head(
        &prior_head,
        next_revision,
        DraftMarkerAdmissionRetainedChargeV1::new(
            1,
            prior_head
                .charge()
                .associations()
                .checked_sub(removed_associations)
                .ok_or(TerminalMutationError::Charge)?,
            0,
        ),
        next_cursor,
    )?;
    let new_head_bytes = encoded_head_record_charge(&prior_head.owner(), &provisional)?;
    let successor_charge = prior_head
        .charge()
        .checked_sub(DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            removed_associations,
            old_head_bytes
                .checked_add(removed_bytes)
                .ok_or(TerminalMutationError::Charge)?,
        ))
        .and_then(|charge| {
            charge.checked_add(DraftMarkerAdmissionRetainedChargeV1::new(
                0,
                0,
                new_head_bytes,
            ))
        })
        .ok_or(TerminalMutationError::Charge)?;
    let head = terminal_head(&prior_head, next_revision, successor_charge, next_cursor)?;
    let capacity = exchange_capacity(capacity, prior_head.charge(), successor_charge)?;
    let capacity_bytes =
        encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &capacity)?;
    checked_draft_marker_admission_command_charge_v1([
        capacity_bytes
            .checked_add(old_head_bytes)
            .and_then(|bytes| bytes.checked_add(receipt_read_bytes))
            .and_then(|bytes| bytes.checked_add(removed_bytes))
            .ok_or(TerminalMutationError::Charge)?,
        capacity_bytes
            .checked_add(encoded_head_record_charge(&head.owner(), &head)?)
            .ok_or(TerminalMutationError::Charge)?,
        removed_bytes,
    ])?;
    if !terminal_receipt_is_exact(&head, receipt_key, &receipt) {
        return Err(TerminalMutationError::Collision);
    }
    Ok(PreparedTerminalMutation {
        capacity,
        head,
        receipt_put: None,
        receipt_delete: None,
        node_deletions: deletions.into_boxed_slice(),
    })
}

fn terminal_head(
    prior: &DraftMarkerAdmissionHeadV1,
    revision: NonZeroU64,
    charge: DraftMarkerAdmissionRetainedChargeV1,
    cursor: DraftMarkerAdmissionCleanupCursorV1,
) -> Result<DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionHeadV1::new(
        prior.owner(),
        revision,
        prior.home_generation(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup,
        prior.request_commitment(),
        prior.custody_commitment(),
        prior.next_page_ordinal(),
        0,
        prior.evidence_eof(),
        None,
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder),
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId),
        prior.occurrence_commitment(),
        0,
        None,
        0,
        charge,
        Some(cursor),
    )
}

fn exchange_capacity(
    prior: DraftMarkerAdmissionCapacityV1,
    prior_charge: DraftMarkerAdmissionRetainedChargeV1,
    successor_charge: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<DraftMarkerAdmissionCapacityV1, TerminalMutationError> {
    let charge = prior
        .charge()
        .checked_sub(prior_charge)
        .and_then(|charge| charge.checked_add(successor_charge))
        .ok_or(TerminalMutationError::Charge)?;
    DraftMarkerAdmissionCapacityV1::new(increment(prior.revision())?, charge).map_err(Into::into)
}

fn increment(value: NonZeroU64) -> Result<NonZeroU64, TerminalMutationError> {
    value
        .get()
        .checked_add(1)
        .and_then(NonZeroU64::new)
        .ok_or(TerminalMutationError::Charge)
}

fn node_range(
    owner: DraftMarkerAdmissionOwnerV1,
    after: Option<DraftMarkerAdmissionNodeKeyV1>,
) -> CursorRange<DraftMarkerAdmissionNodeKeyV1> {
    let last = node_last(owner);
    match after {
        Some(after) => CursorRange::after(after, last),
        None => CursorRange::closed(node_first(owner), last),
    }
}

#[derive(Clone, Copy)]
pub(super) enum TerminalMutationFailureClass {
    Retryable,
    Collision,
    Rejected,
    Unavailable,
}

pub(super) fn classify_terminal_mutation_failure(
    error: &CommandError,
) -> TerminalMutationFailureClass {
    let source = match error {
        CommandError::ContributorValidation { source, .. }
        | CommandError::ContributorReservation { source, .. }
        | CommandError::ContributorAssembly { source, .. } => Some(source.as_ref()),
        _ => None,
    };
    match source.and_then(|source| source.downcast_ref::<TerminalMutationError>()) {
        Some(TerminalMutationError::Collision | TerminalMutationError::Authority) => {
            TerminalMutationFailureClass::Collision
        }
        Some(TerminalMutationError::AlreadyCompact) => TerminalMutationFailureClass::Rejected,
        Some(TerminalMutationError::Read(_)) => TerminalMutationFailureClass::Unavailable,
        Some(
            TerminalMutationError::Build(_)
            | TerminalMutationError::Schema(_)
            | TerminalMutationError::Charge,
        ) => TerminalMutationFailureClass::Rejected,
        None => TerminalMutationFailureClass::Retryable,
    }
}
