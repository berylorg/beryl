use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeStore, MutationBuilder,
    ReconciliationReservation, ReconciliationResolution,
};

use crate::domain::SyndicDomain;
use crate::draft_piece::{
    DraftPieceBuildRecordV1, DraftPieceBuildsFamily, DraftPieceDigestV1, DraftPieceOperationIdV1,
    DraftPieceSettlementKeyV1, DraftPieceSettlementOutcomeV1, DraftPieceSettlementV1,
    DraftPieceSettlementsFamily, checked_draft_marker_admission_command_charge_v1,
    encoded_capacity_record_charge, encoded_head_record_charge, settlement_closure_is_exact,
    settlement_terminal_build_is_exact,
};
use crate::mutation::{point, required};
use crate::{SyndicMutationError, SyndicReadError, SyndicStorage};

use super::super::*;
use super::model::DraftMarkerWriterAdmissionV1;
use super::settlement::staging_terminal_command;
use crate::draft_piece::admission::terminal::closure::terminal_receipt_is_exact;

#[derive(Clone)]
struct ReleaseSettledWriterMutation {
    settlement: DraftPieceSettlementV1,
}

struct PreparedSettledWriterRelease {
    owner: DraftMarkerAdmissionOwnerV1,
    capacity: DraftMarkerAdmissionCapacityV1,
}

impl SyndicStorage {
    pub(crate) fn release_draft_marker_writer_terminal(
        &self,
        store: &HomeStore,
        admission: DraftMarkerWriterAdmissionV1,
        terminal_digest: DraftPieceDigestV1,
    ) -> Result<(), SyndicReadError> {
        let owner = admission.binding().owner();
        let head = self
            .point::<DraftMarkerAdmissionHeadsFamily>(
                store,
                owner,
                crate::draft_piece::point_limit(),
            )?
            .ok_or(SyndicReadError::Invariant(
                "draft-marker writer head missing",
            ))?;
        let command = staging_terminal_command(owner, terminal_digest);
        let key = DraftMarkerAdmissionReceiptKeyV1::new(owner, command);
        let receipt = self.point::<DraftMarkerAdmissionReceiptsFamily>(
            store,
            key,
            crate::draft_piece::point_limit(),
        )?;
        if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::TerminalCleanup
            || head.home_generation() != admission.binding().home_generation()
            || head.source_root().count() != 0
            || head.target_root().count() != 0
            || head.remaining_builder_count() != 0
            || head.occurrence_commitment() != admission.binding().occurrence_commitment()
            || receipt
                .as_ref()
                .is_none_or(|receipt| !terminal_receipt_is_exact(&head, key, receipt))
        {
            return Err(SyndicReadError::Invariant(
                "draft-marker writer terminal closure disagrees",
            ));
        }
        store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.resolve_writer_terminal(owner)
            })
            .map_err(|_| SyndicReadError::Invariant("draft-marker attachment unavailable"))?
            .map_err(|_| SyndicReadError::Invariant("draft-marker reservation release failed"))
    }

    pub(crate) fn release_draft_marker_writer_for_settlement(
        &self,
        store: &HomeStore,
        settlement: &DraftPieceSettlementV1,
    ) -> Result<(), SyndicReadError> {
        let Some(admission) = settlement
            .terminal_source()
            .and_then(DraftPieceBuildRecordV1::writer_admission)
        else {
            return Ok(());
        };
        if !matches!(
            settlement.outcome(),
            DraftPieceSettlementOutcomeV1::Committed { .. }
        ) {
            return self.release_draft_marker_writer_terminal(
                store,
                admission,
                settlement.terminal_receipt().digest(),
            );
        }
        self.reclaim_settled_draft_marker_writer(store, settlement.clone(), false)
    }

    pub fn release_settled_draft_marker_writer(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
    ) -> Result<(), SyndicReadError> {
        let key = DraftPieceSettlementKeyV1::new(
            owner.draft_id(),
            owner.session_id(),
            DraftPieceOperationIdV1::from_bytes(*owner.operation_id().as_bytes()),
        );
        let settlement = self
            .point::<DraftPieceSettlementsFamily>(store, key, crate::draft_piece::point_limit())?
            .ok_or(SyndicReadError::Invariant(
                "draft-marker writer settlement is missing",
            ))?;
        self.reclaim_settled_draft_marker_writer(store, settlement, true)
    }

    fn reclaim_settled_draft_marker_writer(
        &self,
        store: &HomeStore,
        settlement: DraftPieceSettlementV1,
        exact_once: bool,
    ) -> Result<(), SyndicReadError> {
        let admission = settlement
            .terminal_source()
            .and_then(DraftPieceBuildRecordV1::writer_admission)
            .ok_or(SyndicReadError::Invariant(
                "draft-marker writer settlement authority is missing",
            ))?;
        let owner = admission.binding().owner();
        let mutation = ReleaseSettledWriterMutation { settlement };
        let mut retry = true;
        loop {
            let target_selected =
                match store.execute_current(self.handle.current_command(mutation.clone())) {
                    CommandOutcome::NotCommitted { .. } => false,
                    CommandOutcome::Committed { .. } => true,
                    CommandOutcome::Indeterminate { reconciliation, .. } => {
                        match store
                            .reconcile(&reconciliation.install_and_handle())
                            .map_err(|_| {
                                SyndicReadError::Invariant(
                                    "draft-marker writer release reconciliation failed",
                                )
                            })? {
                            ReconciliationResolution::ExactOld => false,
                            ReconciliationResolution::ExactNew { .. } => true,
                            ReconciliationResolution::ExactSuccessor { .. }
                            | ReconciliationResolution::Collision => {
                                return Err(SyndicReadError::Invariant(
                                    "draft-marker writer release collided",
                                ));
                            }
                        }
                    }
                };
            let head = self.point::<DraftMarkerAdmissionHeadsFamily>(
                store,
                owner,
                crate::draft_piece::point_limit(),
            )?;
            if head.is_none() {
                break;
            }
            if target_selected || !retry {
                return Err(SyndicReadError::Invariant(
                    "draft-marker writer durable release disagrees",
                ));
            }
            retry = false;
        }
        self.release_draft_marker_writer_attachment(store, owner, exact_once)
    }

    fn release_draft_marker_writer_attachment(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        exact_once: bool,
    ) -> Result<(), SyndicReadError> {
        store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                if exact_once {
                    attachment.release_terminal_once(owner)
                } else {
                    attachment.resolve_terminal(owner, true, false)
                }
            })
            .map_err(|_| SyndicReadError::Invariant("draft-marker attachment unavailable"))?
            .map_err(|_| SyndicReadError::Invariant("draft-marker reservation release failed"))
    }
}

impl DomainMutation<SyndicDomain> for ReleaseSettledWriterMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedSettledWriterRelease>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let settlement = required::<DraftPieceSettlementsFamily>(reader, &self.settlement.key())?;
        let terminal = settlement
            .terminal_source()
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let admission = terminal
            .writer_admission()
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let owner = admission.binding().owner();
        let stored_build = required::<DraftPieceBuildsFamily>(reader, &settlement.key())?;
        if settlement != self.settlement
            || !matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Committed { .. }
            )
            || !settlement_closure_is_exact(&settlement)
            || !settlement_terminal_build_is_exact(&settlement, Some(&stored_build))
            || terminal.writer_admission() != Some(admission)
            || !admission.is_empty()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let Some(head) = point::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)? else {
            return Ok(None);
        };
        if !draft_marker_writer_settlement_is_exact_v1(reader, admission, true)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let prior_capacity = required::<DraftMarkerAdmissionCapacityFamily>(
            reader,
            &DraftMarkerAdmissionCapacityKeyV1,
        )?;
        let charge = prior_capacity
            .charge()
            .checked_sub(head.charge())
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let capacity = DraftMarkerAdmissionCapacityV1::new(
            NonZeroU64::new(
                prior_capacity
                    .revision()
                    .get()
                    .checked_add(1)
                    .ok_or(SyndicMutationError::IdentityCollision)?,
            )
            .ok_or(SyndicMutationError::IdentityCollision)?,
            charge,
        )
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
        let source_capacity_bytes =
            encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &prior_capacity)
                .map_err(|_| SyndicMutationError::IdentityCollision)?;
        let target_capacity_bytes =
            encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, &capacity)
                .map_err(|_| SyndicMutationError::IdentityCollision)?;
        let head_bytes = encoded_head_record_charge(&owner, &head)
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        checked_draft_marker_admission_command_charge_v1([
            source_capacity_bytes
                .checked_add(head_bytes)
                .ok_or(SyndicMutationError::IdentityCollision)?,
            target_capacity_bytes,
            head_bytes,
        ])
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
        Ok(Some(PreparedSettledWriterRelease { owner, capacity }))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.delete::<DraftMarkerAdmissionHeadsCodec>(&prepared.owner)?;
            mutations.put::<DraftMarkerAdmissionCapacityCodec>(
                &DraftMarkerAdmissionCapacityKeyV1,
                &prepared.capacity,
            )?;
        }
        Ok(())
    }
}
