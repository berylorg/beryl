use std::{any::TypeId, error::Error};

use beryl_model::{DomainRevision, HomeRevision};
use thiserror::Error;

use crate::{HealthGateError, HomeGeneration, domain::StoreInstanceId, health::FailureSeverity};

use super::{
    HomeProofCommand, HomeProofProtocol, MAX_PROOF_ROLES, PreparedProofRole, ProofCorrelation,
};

#[derive(Clone, Copy, Eq, PartialEq)]
struct ProofRoleFence {
    slot: usize,
    revision: DomainRevision,
}

#[must_use = "a proof source receipt consumer binds one exact sealed proof receipt"]
pub struct ProofReceiptConsumer<P: HomeProofProtocol> {
    store: StoreInstanceId,
    generation: HomeGeneration,
    home_revision: HomeRevision,
    protocol: TypeId,
    protocol_id: u64,
    operation_id: u64,
    command_id: u64,
    source: ProofRoleFence,
    witness_count: u8,
    witnesses: [Option<ProofRoleFence>; MAX_PROOF_ROLES - 1],
    correlation: ProofCorrelation<P>,
}

impl<P: HomeProofProtocol> std::fmt::Debug for ProofReceiptConsumer<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProofReceiptConsumer")
            .field("generation", &self.generation)
            .field("home_revision", &self.home_revision)
            .field("witness_count", &self.witness_count)
            .finish_non_exhaustive()
    }
}

#[must_use = "proof receipts must be consumed by their exact source receipt consumer"]
pub struct HomeProofReceipt<P: HomeProofProtocol> {
    store: StoreInstanceId,
    generation: HomeGeneration,
    home_revision: HomeRevision,
    protocol: TypeId,
    protocol_id: u64,
    operation_id: u64,
    command_id: u64,
    source: ProofRoleFence,
    witness_count: u8,
    witnesses: [Option<ProofRoleFence>; MAX_PROOF_ROLES - 1],
    correlation: ProofCorrelation<P>,
}

impl<P: HomeProofProtocol> std::fmt::Debug for HomeProofReceipt<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = self.correlation;
        formatter
            .debug_struct("HomeProofReceipt")
            .field("generation", &self.generation)
            .field("home_revision", &self.home_revision)
            .field("witness_count", &self.witness_count)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum ProofReceiptError {
    #[error(transparent)]
    HealthGate(#[from] HealthGateError),
    #[error("proof receipt validation could not confirm storage health: {source}")]
    StorageHealth {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    #[error("proof receipt belongs to another or obsolete Beryl-home generation")]
    StaleOrForeign,
    #[error("proof receipt does not match its sealed source, home, and role fences")]
    SourceFenceMismatch,
    #[error("proof receipt does not match its sealed source correlation")]
    CorrelationMismatch,
}

impl<P: HomeProofProtocol> HomeProofReceipt<P> {
    pub(crate) fn new(
        store: StoreInstanceId,
        generation: HomeGeneration,
        command_id: u64,
        home_revision: HomeRevision,
        source: &PreparedProofRole<'_, P>,
        witnesses: &[PreparedProofRole<'_, P>],
        correlation: ProofCorrelation<P>,
    ) -> Self {
        let mut receipt_witnesses = [None; MAX_PROOF_ROLES - 1];
        for (index, witness) in witnesses.iter().enumerate() {
            receipt_witnesses[index] = Some(ProofRoleFence {
                slot: witness.plan.slot,
                revision: witness.revision,
            });
        }
        Self {
            store,
            generation,
            home_revision,
            protocol: TypeId::of::<P>(),
            protocol_id: P::PROTOCOL_ID,
            operation_id: P::OPERATION_ID,
            command_id,
            source: ProofRoleFence {
                slot: source.plan.slot,
                revision: source.revision,
            },
            witness_count: u8::try_from(witnesses.len())
                .expect("fixed proof witness count always fits u8"),
            witnesses: receipt_witnesses,
            correlation,
        }
    }
}

impl<P: HomeProofProtocol> ProofReceiptConsumer<P> {
    pub(super) fn from_command(command: &HomeProofCommand<P>, command_id: u64) -> Self {
        let mut witnesses = [None; MAX_PROOF_ROLES - 1];
        for (index, witness) in command.witnesses.iter().enumerate() {
            witnesses[index] = Some(ProofRoleFence {
                slot: witness.plan.slot,
                revision: witness.expected_revision,
            });
        }
        Self {
            store: command.source.plan.store,
            generation: command.expected_generation,
            home_revision: command.expected_home_revision,
            protocol: TypeId::of::<P>(),
            protocol_id: P::PROTOCOL_ID,
            operation_id: P::OPERATION_ID,
            command_id,
            source: ProofRoleFence {
                slot: command.source.plan.slot,
                revision: command.source.expected_revision,
            },
            witness_count: u8::try_from(command.witnesses.len())
                .expect("fixed proof witness count always fits u8"),
            witnesses,
            correlation: ProofCorrelation::from_bytes(command.source.expected_correlation)
                .expect("validated proof command has the source correlation's fixed inline size"),
        }
    }

    fn matches_receipt(&self, receipt: &HomeProofReceipt<P>) -> bool {
        self.store == receipt.store
            && self.generation == receipt.generation
            && self.home_revision == receipt.home_revision
            && self.protocol == receipt.protocol
            && self.protocol_id == receipt.protocol_id
            && self.operation_id == receipt.operation_id
            && self.command_id == receipt.command_id
            && self.source == receipt.source
            && self.witness_count == receipt.witness_count
            && self.witnesses == receipt.witnesses
    }
}

impl crate::HomeStore {
    fn validate_proof_receipt<P: HomeProofProtocol>(
        &self,
        receipt: &HomeProofReceipt<P>,
    ) -> Result<(), ProofReceiptError> {
        let admission = self.health.admit()?;
        let generation_guard = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(ProofReceiptError::GenerationPoisoned);
            }
        };
        let generation = match generation_guard.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(ProofReceiptError::GenerationPoisoned);
            }
        };
        if receipt.generation != admission.generation()
            || receipt.store != generation.instance_id
            || receipt.protocol != TypeId::of::<P>()
            || receipt.protocol_id != P::PROTOCOL_ID
            || receipt.operation_id != P::OPERATION_ID
        {
            return Err(ProofReceiptError::StaleOrForeign);
        }
        admission.confirm()?;
        Ok(())
    }

    pub fn consume_proof_receipt<P: HomeProofProtocol>(
        &self,
        consumer: ProofReceiptConsumer<P>,
        receipt: HomeProofReceipt<P>,
    ) -> Result<(), ProofReceiptError> {
        self.validate_proof_receipt(&receipt)?;
        if !consumer.matches_receipt(&receipt) {
            return Err(ProofReceiptError::SourceFenceMismatch);
        }
        if !consumer.correlation.agrees_with(receipt.correlation) {
            return Err(ProofReceiptError::CorrelationMismatch);
        }
        Ok(())
    }
}
