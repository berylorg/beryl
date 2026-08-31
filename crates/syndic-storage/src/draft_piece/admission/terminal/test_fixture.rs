use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    MutationBuildError, MutationBuilder, ReadError, ReconciliationReservation,
};

use crate::{
    SyndicStorage,
    domain::SyndicDomain,
    draft_piece::{
        DraftMarkerAdmissionCapacityCodec, DraftMarkerAdmissionCapacityFamily,
        DraftMarkerAdmissionCapacityKeyV1, DraftMarkerAdmissionCapacityV1,
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionReceiptKeyV1,
        DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReplayReceiptV1,
        DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionSchemaErrorV1,
        encoded_head_record_charge,
    },
};

use super::closure::{
    TerminalClosureError, read_terminal_closure, validate_compact_terminal_charge,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionTerminalReceiptFaultV1 {
    Missing,
    Mismatched,
    Extra,
    ChargeMismatch,
}

impl SyndicStorage {
    pub fn inject_draft_marker_admission_terminal_receipt_fault_for_test(
        &self,
        store: &beryl_home_store::HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        fault: DraftMarkerAdmissionTerminalReceiptFaultV1,
    ) -> CommandOutcome {
        store.execute_current(
            self.handle
                .current_command(TerminalReceiptFaultMutation { owner, fault }),
        )
    }
}

pub(crate) struct TerminalReceiptFaultMutation {
    owner: DraftMarkerAdmissionOwnerV1,
    fault: DraftMarkerAdmissionTerminalReceiptFaultV1,
}

pub(crate) struct PreparedTerminalReceiptFaultMutation {
    delete: Option<DraftMarkerAdmissionReceiptKeyV1>,
    put: Option<(
        DraftMarkerAdmissionReceiptKeyV1,
        DraftMarkerAdmissionReplayReceiptV1,
    )>,
    head_put: Option<DraftMarkerAdmissionHeadV1>,
    capacity_put: Option<DraftMarkerAdmissionCapacityV1>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TerminalReceiptFaultMutationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Schema(#[from] DraftMarkerAdmissionSchemaErrorV1),
    #[error("draft-marker terminal receipt fault fixture requires an exact closure")]
    Authority,
    #[error("draft-marker terminal receipt fault fixture charge overflow")]
    Charge,
}

impl DomainCallbackError for TerminalReceiptFaultMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

impl DomainMutation<SyndicDomain> for TerminalReceiptFaultMutation {
    type Error = TerminalReceiptFaultMutationError;
    type Prepared = PreparedTerminalReceiptFaultMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let head = reader
            .point::<DraftMarkerAdmissionHeadsCodec>(
                &self.owner,
                crate::codec::family_point_limit::<DraftMarkerAdmissionHeadsFamily>(),
            )?
            .ok_or(TerminalReceiptFaultMutationError::Authority)?;
        let closure = read_terminal_closure(reader, &head).map_err(|error| match error {
            TerminalClosureError::Read(error) => TerminalReceiptFaultMutationError::Read(error),
            TerminalClosureError::Invalid => TerminalReceiptFaultMutationError::Authority,
        })?;
        match self.fault {
            DraftMarkerAdmissionTerminalReceiptFaultV1::Missing => {
                Ok(PreparedTerminalReceiptFaultMutation {
                    delete: Some(closure.key),
                    put: None,
                    head_put: None,
                    capacity_put: None,
                })
            }
            DraftMarkerAdmissionTerminalReceiptFaultV1::Mismatched => {
                let mut source_head_bytes = closure.receipt.source_head_bytes().to_vec();
                source_head_bytes.push(0);
                let receipt = replacement_receipt(
                    &head,
                    closure.key.command_id(),
                    &closure.receipt,
                    source_head_bytes.into_boxed_slice(),
                )?;
                Ok(PreparedTerminalReceiptFaultMutation {
                    delete: None,
                    put: Some((closure.key, receipt)),
                    head_put: None,
                    capacity_put: None,
                })
            }
            DraftMarkerAdmissionTerminalReceiptFaultV1::Extra => {
                let mut bytes = *closure.key.command_id().as_bytes();
                bytes[bytes.len() - 1] ^= 1;
                let command = DraftMarkerAdmissionCommandIdV1::from_bytes(bytes);
                let key = DraftMarkerAdmissionReceiptKeyV1::new(self.owner, command);
                let receipt = replacement_receipt(
                    &head,
                    command,
                    &closure.receipt,
                    closure.receipt.source_head_bytes().into(),
                )?;
                Ok(PreparedTerminalReceiptFaultMutation {
                    delete: None,
                    put: Some((key, receipt)),
                    head_put: None,
                    capacity_put: None,
                })
            }
            DraftMarkerAdmissionTerminalReceiptFaultV1::ChargeMismatch => {
                validate_compact_terminal_charge(&head, &closure)
                    .map_err(|_| TerminalReceiptFaultMutationError::Authority)?;
                let (head, capacity) = charge_mismatch(reader, head, closure.encoded_bytes)?;
                Ok(PreparedTerminalReceiptFaultMutation {
                    delete: None,
                    put: None,
                    head_put: Some(head),
                    capacity_put: Some(capacity),
                })
            }
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(2)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(key) = prepared.delete {
            mutations.delete::<DraftMarkerAdmissionReceiptsCodec>(&key)?;
        }
        if let Some((key, receipt)) = prepared.put {
            mutations.put::<DraftMarkerAdmissionReceiptsCodec>(&key, &receipt)?;
        }
        if let Some(head) = prepared.head_put {
            mutations.put::<DraftMarkerAdmissionHeadsCodec>(&head.owner(), &head)?;
        }
        if let Some(capacity) = prepared.capacity_put {
            mutations.put::<DraftMarkerAdmissionCapacityCodec>(
                &DraftMarkerAdmissionCapacityKeyV1,
                &capacity,
            )?;
        }
        Ok(())
    }
}

fn charge_mismatch(
    reader: &DomainReader<'_, SyndicDomain>,
    head: DraftMarkerAdmissionHeadV1,
    receipt_bytes: u64,
) -> Result<
    (DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionCapacityV1),
    TerminalReceiptFaultMutationError,
> {
    let capacity = reader
        .point::<DraftMarkerAdmissionCapacityCodec>(
            &DraftMarkerAdmissionCapacityKeyV1,
            crate::codec::family_point_limit::<DraftMarkerAdmissionCapacityFamily>(),
        )?
        .ok_or(TerminalReceiptFaultMutationError::Authority)?;
    let exact_bytes = encoded_head_record_charge(&head.owner(), &head)?
        .checked_add(receipt_bytes)
        .ok_or(TerminalReceiptFaultMutationError::Charge)?;
    let mut declared_bytes = exact_bytes
        .checked_add(1)
        .ok_or(TerminalReceiptFaultMutationError::Charge)?;
    let mut mismatched = head_with_charge(
        &head,
        DraftMarkerAdmissionRetainedChargeV1::new(1, 0, declared_bytes),
    )?;
    let first_physical_bytes = encoded_head_record_charge(&mismatched.owner(), &mismatched)?
        .checked_add(receipt_bytes)
        .ok_or(TerminalReceiptFaultMutationError::Charge)?;
    if first_physical_bytes == declared_bytes {
        declared_bytes = declared_bytes
            .checked_add(1)
            .ok_or(TerminalReceiptFaultMutationError::Charge)?;
        mismatched = head_with_charge(
            &head,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 0, declared_bytes),
        )?;
    }
    let aggregate = capacity
        .charge()
        .checked_sub(head.charge())
        .and_then(|charge| charge.checked_add(mismatched.charge()))
        .ok_or(TerminalReceiptFaultMutationError::Charge)?;
    let capacity_revision = capacity
        .revision()
        .get()
        .checked_add(1)
        .and_then(NonZeroU64::new)
        .ok_or(TerminalReceiptFaultMutationError::Charge)?;
    let capacity = DraftMarkerAdmissionCapacityV1::new(capacity_revision, aggregate)?;
    Ok((mismatched, capacity))
}

fn head_with_charge(
    prior: &DraftMarkerAdmissionHeadV1,
    charge: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionHeadV1::new(
        prior.owner(),
        prior.revision(),
        prior.home_generation(),
        prior.lifecycle(),
        prior.request_commitment(),
        prior.custody_commitment(),
        prior.next_page_ordinal(),
        prior.ingestion_association_cursor(),
        prior.evidence_eof(),
        prior.selected_receipt(),
        prior.source_root(),
        prior.target_root(),
        prior.occurrence_commitment(),
        prior.unassigned_count(),
        prior.assignment_continuation(),
        prior.remaining_builder_count(),
        charge,
        prior.cleanup_cursor(),
    )
}

fn replacement_receipt(
    head: &DraftMarkerAdmissionHeadV1,
    command: DraftMarkerAdmissionCommandIdV1,
    prior: &DraftMarkerAdmissionReplayReceiptV1,
    source_head_bytes: Box<[u8]>,
) -> Result<DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionReplayReceiptV1::new(
        head.owner(),
        command,
        prior.page_ordinal(),
        prior.request_commitment(),
        source_head_bytes,
        prior.target_head_bytes(),
        prior.source_before(),
        prior.source_after(),
        prior.target_before(),
        prior.target_after(),
        prior.retained_predecessor_nodes(),
        prior.transition(),
    )
}
