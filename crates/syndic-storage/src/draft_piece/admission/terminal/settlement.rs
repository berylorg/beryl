use std::num::NonZeroU64;

use beryl_home_store::{
    CurrentDomainCommand, CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError,
    DomainCallbackSource, DomainMutation, DomainReader, MutationBuildError, MutationBuilder,
    ReadError, ReconciliationReservation,
};

use crate::{
    SyndicStorage,
    codec::family_point_limit,
    domain::SyndicDomain,
    draft_piece::{
        DraftMarkerAdmissionCapacityCodec, DraftMarkerAdmissionCapacityFamily,
        DraftMarkerAdmissionCapacityKeyV1, DraftMarkerAdmissionCapacityV1,
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionDigestV1,
        DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodesCodec,
        DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionReceiptKeyV1,
        DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionSchemaErrorV1,
        checked_draft_marker_admission_command_charge_v1, encoded_capacity_record_charge,
        encoded_head_record_charge,
    },
};

use super::closure::{
    TERMINAL_READ_BYTES, TerminalClosureError, node_first, node_last, read_terminal_closure,
    validate_compact_terminal_charge,
};

pub(crate) struct DraftMarkerAdmissionSettlementAuthorityV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    terminal_command: DraftMarkerAdmissionCommandIdV1,
    terminal_digest: DraftMarkerAdmissionDigestV1,
}

impl DraftMarkerAdmissionSettlementAuthorityV1 {
    pub(crate) const fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        terminal_command: DraftMarkerAdmissionCommandIdV1,
        terminal_digest: DraftMarkerAdmissionDigestV1,
    ) -> Self {
        Self {
            owner,
            terminal_command,
            terminal_digest,
        }
    }
}

pub(crate) fn settlement_transfer_command(
    storage: &SyndicStorage,
    authority: DraftMarkerAdmissionSettlementAuthorityV1,
) -> CurrentDomainCommand {
    storage
        .handle
        .current_command(SettlementTransferMutation { authority })
}

pub(crate) struct SettlementTransferMutation {
    authority: DraftMarkerAdmissionSettlementAuthorityV1,
}

pub(crate) struct PreparedSettlementTransferMutation {
    capacity: DraftMarkerAdmissionCapacityV1,
    owner: DraftMarkerAdmissionOwnerV1,
    receipt_key: DraftMarkerAdmissionReceiptKeyV1,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SettlementTransferMutationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Schema(#[from] DraftMarkerAdmissionSchemaErrorV1),
    #[error("draft-marker settlement transfer authority disagrees")]
    Authority,
    #[error("draft-marker settlement transfer charge disagrees")]
    Charge,
}

impl DomainCallbackError for SettlementTransferMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

impl DomainMutation<SyndicDomain> for SettlementTransferMutation {
    type Error = SettlementTransferMutationError;
    type Prepared = PreparedSettlementTransferMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let capacity = reader
            .point::<DraftMarkerAdmissionCapacityCodec>(
                &DraftMarkerAdmissionCapacityKeyV1,
                family_point_limit::<DraftMarkerAdmissionCapacityFamily>(),
            )?
            .ok_or(SettlementTransferMutationError::Authority)?;
        let head = reader
            .point::<DraftMarkerAdmissionHeadsCodec>(
                &self.authority.owner,
                family_point_limit::<DraftMarkerAdmissionHeadsFamily>(),
            )?
            .ok_or(SettlementTransferMutationError::Authority)?;
        let closure = read_terminal_closure(reader, &head).map_err(|error| match error {
            TerminalClosureError::Read(error) => SettlementTransferMutationError::Read(error),
            TerminalClosureError::Invalid => SettlementTransferMutationError::Authority,
        })?;
        validate_compact_terminal_charge(&head, &closure).map_err(|error| match error {
            TerminalClosureError::Read(error) => SettlementTransferMutationError::Read(error),
            TerminalClosureError::Invalid => SettlementTransferMutationError::Authority,
        })?;
        let nodes = reader.cursor::<DraftMarkerAdmissionNodesCodec>(
            &CursorRange::closed(
                node_first(self.authority.owner),
                node_last(self.authority.owner),
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(1, TERMINAL_READ_BYTES)
                .expect("draft-marker settlement node limits are nonzero"),
        )?;
        if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::TerminalCleanup
            || head.source_root().count() != 0
            || head.target_root().count() != 0
            || !nodes.records().is_empty()
            || closure.key.command_id() != self.authority.terminal_command
            || closure.receipt.digest() != self.authority.terminal_digest
        {
            return Err(SettlementTransferMutationError::Authority);
        }
        let charge = capacity
            .charge()
            .checked_sub(head.charge())
            .ok_or(SettlementTransferMutationError::Charge)?;
        let capacity_revision = capacity
            .revision()
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(SettlementTransferMutationError::Charge)?;
        let capacity = DraftMarkerAdmissionCapacityV1::new(capacity_revision, charge)?;
        let capacity_bytes =
            encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &capacity)?;
        let head_bytes = encoded_head_record_charge(&head.owner(), &head)?;
        checked_draft_marker_admission_command_charge_v1([
            capacity_bytes
                .checked_add(head_bytes)
                .and_then(|bytes| bytes.checked_add(closure.encoded_bytes))
                .ok_or(SettlementTransferMutationError::Charge)?,
            capacity_bytes,
            head_bytes
                .checked_add(closure.encoded_bytes)
                .ok_or(SettlementTransferMutationError::Charge)?,
        ])?;
        Ok(PreparedSettlementTransferMutation {
            capacity,
            owner: self.authority.owner,
            receipt_key: closure.key,
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<DraftMarkerAdmissionHeadsCodec>(&prepared.owner)?;
        mutations.delete::<DraftMarkerAdmissionReceiptsCodec>(&prepared.receipt_key)?;
        mutations.put::<DraftMarkerAdmissionCapacityCodec>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.capacity,
        )?;
        Ok(())
    }
}
